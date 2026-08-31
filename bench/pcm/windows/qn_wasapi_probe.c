/*
 * E10-A0 native Windows WASAPI probe — bench-only, NOT shipping code.
 *
 * Answers, for each active render endpoint:
 *   - friendly name / hashed stable endpoint id / state
 *   - mix format (rate, channels, sample container) and engine period
 *   - Path 1 (app BYPASS): does shared Initialize at source-rate Float32
 *     stereo succeed with no SRC flag?
 *   - Path 2 (Windows-owned SRC): shared Initialize with
 *     AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | SRC_DEFAULT_QUALITY
 *   - Path 3 (source-rate device format): IsFormatSupported(EXCLUSIVE)
 *     at source-rate Float32 stereo
 *   - IsFormatSupported(SHARED) per rate (S_OK / S_FALSE closest)
 *   - reopen/reconfigure cost: repeated Activate -> GetMixFormat ->
 *     IsFormatSupported -> Initialize -> Start -> Stop -> Release cycles
 *     for 44100 -> 48000 -> 44100 (phase-separated, QPC-timed)
 *
 * Primary rates 44100/48000/96000; also 88200/176400/192000 when cheap.
 * Output: three machine JSON files written to argv[1] directory:
 *   a0-windows-endpoints.json  a0-format-support.json  a0-reopen.json
 *
 * No audible test signals are emitted (silence only). No bit-perfect
 * claim: format acceptance is not absence of driver/device DSP.
 */
#define WIN32_LEAN_AND_MEAN
#define _CRT_SECURE_NO_WARNINGS
#include <windows.h>
#include <mmdeviceapi.h>
#include <audioclient.h>
#include <avrt.h>
#include <functiondiscoverykeys_devpkey.h>
#include <propkey.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <wchar.h>
#include <math.h>

/* GUID constants defined locally: uuid.lib linkage of the SDK GUIDs is
 * unreliable across SDK/kit versions for a standalone probe. Values are
 * the canonical published ones (from mmdeviceapi.h/audioclient.h). */
static const CLSID kCLSID_MMDeviceEnumerator =
    {0xBCDE0395, 0xE52F, 0x467C, {0x8E, 0x3D, 0xC4, 0x57, 0x92, 0x91, 0x69, 0x2E}};
static const IID kIID_IMMDeviceEnumerator =
    {0xA95664D2, 0x9614, 0x4F35, {0xA7, 0x46, 0xDE, 0x8D, 0xB6, 0x36, 0x17, 0xE6}};
static const IID kIID_IAudioClient =
    {0x1CB9AD4C, 0xDBFA, 0x4C32, {0xB1, 0x78, 0xC2, 0xF5, 0x68, 0xA7, 0x03, 0xB2}};

/* ---------------------------------------------------------------- */
/* helpers                                                           */
/* ---------------------------------------------------------------- */

static void hex_sha1(const void *data, size_t len, char out[41]) {
    /* Windows CNG-free fallback is overkill; use a deterministic FNV-1a
     * 64-bit digest printed as hex — this is a stable id hash for
     * privacy, not a cryptographic claim. */
    uint64_t h = 0xcbf29ce484222325ULL;
    const unsigned char *p = (const unsigned char *)data;
    size_t i;
    for (i = 0; i < len; i++) {
        h ^= p[i];
        h *= 0x100000001b3ULL;
    }
    snprintf(out, 41, "%016llx", (unsigned long long)h);
}

static void json_escape(FILE *f, const wchar_t *s) {
    fputc('"', f);
    for (; s && *s; s++) {
        if (*s == L'"' || *s == L'\\') fputc((int)*s, f);
        else if (*s >= 0x20) {
            /* narrow print of BMP code points; surrogate pairs printed
             * raw (UTF-8 locale in modern Windows console) */
            unsigned int c = (unsigned int)*s;
            if (c < 0x80) fputc((int)c, f);
            else fprintf(f, "\\u%04x", c);
        }
    }
    fputc('"', f);
}

static double qpc_s(void) {
    static LARGE_INTEGER freq;
    LARGE_INTEGER t;
    if (!freq.QuadPart) QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&t);
    return (double)t.QuadPart / (double)freq.QuadPart;
}

static double arr_median(double *v, int n) {
    int i, j;
    for (i = 1; i < n; i++) {
        double x = v[i];
        j = i - 1;
        while (j >= 0 && v[j] > x) { v[j + 1] = v[j]; j--; }
        v[j + 1] = x;
    }
    return v[n / 2];
}

static const char *hres_name(HRESULT hr) {
    switch (hr) {
    case S_OK: return "S_OK";
    case S_FALSE: return "S_FALSE";
    case E_INVALIDARG: return "E_INVALIDARG";
    case E_POINTER: return "E_POINTER";
    case AUDCLNT_E_UNSUPPORTED_FORMAT: return "AUDCLNT_E_UNSUPPORTED_FORMAT";
    case AUDCLNT_E_DEVICE_INVALIDATED: return "AUDCLNT_E_DEVICE_INVALIDATED";
    case AUDCLNT_E_ALREADY_INITIALIZED: return "AUDCLNT_E_ALREADY_INITIALIZED";
    case AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED: return "AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED";
    case AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED: return "AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED";
    case AUDCLNT_E_SERVICE_NOT_RUNNING: return "AUDCLNT_E_SERVICE_NOT_RUNNING";
    case E_ACCESSDENIED: return "E_ACCESSDENIED";
    case REGDB_E_CLASSNOTREG: return "REGDB_E_CLASSNOTREG";
    default: return "OTHER";
    }
}

/* ---------------------------------------------------------------- */
/* format building                                                   */
/* ---------------------------------------------------------------- */

static void make_fmt32(WAVEFORMATEXTENSIBLE *wf, DWORD rate, WORD ch) {
    memset(wf, 0, sizeof(*wf));
    wf->Format.wFormatTag = WAVE_FORMAT_EXTENSIBLE;
    wf->Format.nChannels = ch;
    wf->Format.nSamplesPerSec = rate;
    wf->Format.wBitsPerSample = 32;
    wf->Format.nBlockAlign = (WORD)(ch * 4);
    wf->Format.nAvgBytesPerSec = rate * ch * 4;
    wf->Format.cbSize = sizeof(WAVEFORMATEXTENSIBLE) - sizeof(WAVEFORMATEX);
    wf->Samples.wValidBitsPerSample = 32;
    wf->dwChannelMask = ch >= 2 ? (SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT)
                                : SPEAKER_FRONT_CENTER;
    wf->SubFormat = KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
}

static void describe_mix(FILE *f, const WAVEFORMATEX *wf) {
    if (!wf) { fprintf(f, "null"); return; }
    fprintf(f, "{\"rate\": %lu, \"channels\": %u, \"bits\": %u, "
               "\"block_align\": %u, \"tag\": %u",
            (unsigned long)wf->nSamplesPerSec, (unsigned)wf->nChannels,
            (unsigned)wf->wBitsPerSample, (unsigned)wf->nBlockAlign,
            (unsigned)wf->wFormatTag);
    if (wf->wFormatTag == WAVE_FORMAT_EXTENSIBLE && wf->cbSize >= 22) {
        const WAVEFORMATEXTENSIBLE *we = (const WAVEFORMATEXTENSIBLE *)wf;
        fprintf(f, ", \"valid_bits\": %u, \"subtype_float\": %d",
                (unsigned)we->Samples.wValidBitsPerSample,
                IsEqualGUID(we->SubFormat, KSDATAFORMAT_SUBTYPE_IEEE_FLOAT));
    }
    fprintf(f, "}");
}

/* ---------------------------------------------------------------- */
/* per-endpoint probe                                                */
/* ---------------------------------------------------------------- */

typedef struct {
    IMMDevice *dev;
    wchar_t *id;              /* hashed in output */
    wchar_t *friendly;
    wchar_t *desc;
    int is_default;
} endpoint_t;

/* Path probe: activate a FRESH IAudioClient, Initialize once with the
 * given mode/flags/rate, record the HRESULT, then Stop+Reset+Release.
 * Initialize may only be called once per client, so each path gets its
 * own client. */
static const char *path_probe(IMMDevice *dev, AUDCLNT_SHAREMODE mode,
                              DWORD flags, DWORD rate) {
    static char buf[64];
    IAudioClient *ac = NULL;
    WAVEFORMATEXTENSIBLE wf;
    HRESULT hr;
    if (FAILED(dev->Activate(kIID_IAudioClient, CLSCTX_ALL, NULL,
                             (void **)&ac)) || !ac)
        return "ACTIVATE_FAILED";
    make_fmt32(&wf, rate, 2);
    hr = ac->Initialize(mode, flags, 0, 0, (WAVEFORMATEX *)&wf, NULL);
    if (SUCCEEDED(hr)) { ac->Stop(); ac->Reset(); }
    ac->Release();
    snprintf(buf, sizeof(buf), "%s", hres_name(hr));
    return buf;
}

static int probe_endpoint_formats(FILE *ff, endpoint_t *ep) {
    IAudioClient *ac = NULL;
    WAVEFORMATEX *mix = NULL;
    char idhash[41] = "";
    int i;
    HRESULT hr;

    if (ep->id) hex_sha1(ep->id, wcslen(ep->id) * sizeof(wchar_t), idhash);

    hr = ep->dev->Activate(kIID_IAudioClient, CLSCTX_ALL, NULL,
                           (void **)&ac);
    if (FAILED(hr) || !ac) {
        fprintf(ff, "    {\"endpoint_id_hash\": \"%s\", "
                    "\"activate\": \"%s\"}",
                idhash, hres_name(hr));
        return 0;
    }

    /* mix format + engine period (on the same client; GetMixFormat and
     * GetDevicePeriod do not consume the one Initialize) */
    hr = ac->GetMixFormat(&mix);
    fprintf(ff, "    {\"endpoint_id_hash\": \"%s\",\n", idhash);
    fprintf(ff, "     \"mix_format\": ");
    describe_mix(ff, mix);
    fprintf(ff, ",\n");
    if (FAILED(hr)) { ac->Release(); return 0; }
    {
        REFERENCE_TIME def = 0, min = 0;
        hr = ac->GetDevicePeriod(&def, &min);
        fprintf(ff, "     \"engine_period_hns\": {\"default\": %lld, "
                    "\"minimum\": %lld},\n",
                (long long)(SUCCEEDED(hr) ? def : -1),
                (long long)(SUCCEEDED(hr) ? min : -1));
    }

    /* per-rate IsFormatSupported (shared + exclusive) */
    fprintf(ff, "     \"rates\": [\n");
    {
        static const DWORD rates[] = {44100, 48000, 96000,
                                      88200, 176400, 192000};
        for (i = 0; i < (int)(sizeof(rates) / sizeof(rates[0])); i++) {
            WAVEFORMATEXTENSIBLE wf;
            WAVEFORMATEX *closest = NULL;
            HRESULT hsh, hexc;
            make_fmt32(&wf, rates[i], 2);
            hsh = ac->IsFormatSupported(AUDCLNT_SHAREMODE_SHARED,
                                        (WAVEFORMATEX *)&wf, &closest);
            hexc = ac->IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE,
                                         (WAVEFORMATEX *)&wf, NULL);
            fprintf(ff, "      {\"rate\": %lu, \"format\": \"f32le stereo\", "
                        "\"shared_isformat\": \"%s\", "
                        "\"shared_closest_rate\": %lu, "
                        "\"exclusive_isformat\": \"%s\"}%s\n",
                    (unsigned long)rates[i], hres_name(hsh),
                    closest ? (unsigned long)closest->nSamplesPerSec : 0,
                    hres_name(hexc),
                    i < 5 ? "," : "");
            if (closest) CoTaskMemFree(closest);
        }
    }
    fprintf(ff, "     ],\n");
    if (mix) CoTaskMemFree(mix);
    ac->Release();

    /* three independent paths, each on a fresh client */
    fprintf(ff, "     \"paths\": {\n");
    fprintf(ff, "       \"path1_app_bypass_shared_init\": "
                "{\"rate\": 48000, \"result\": \"%s\"},\n",
            path_probe(ep->dev, AUDCLNT_SHAREMODE_SHARED, 0, 48000));
    fprintf(ff, "       \"path2_windows_src_shared_init\": "
                "{\"rate\": 44100, \"flags\": "
                "\"AUTOCONVERTPCM|SRC_DEFAULT_QUALITY\", "
                "\"result\": \"%s\"},\n",
            path_probe(ep->dev, AUDCLNT_SHAREMODE_SHARED,
                       AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM |
                       AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, 44100));
    fprintf(ff, "       \"path3_exclusive_init\": "
                "{\"rate\": 44100, \"result\": \"%s\"}\n",
            path_probe(ep->dev, AUDCLNT_SHAREMODE_EXCLUSIVE, 0, 44100));
    fprintf(ff, "     }\n    }");
    return 1;
}

/* ---------------------------------------------------------------- */
/* reopen cost                                                       */
/* ---------------------------------------------------------------- */

static void run_reopen(FILE *fr, IMMDevice *dev) {
    /* 44100 -> 48000 -> 44100 with repeated native cycles; phase-separated
     * QPC timings. We time a full client cycle per rate per iteration:
     * Activate, GetMixFormat, IsFormatSupported, Initialize(shared,
     * AUTOCONVERTPCM), Start, Stop, Release. */
    const int iters = 30;
    const DWORD rates[3] = {44100, 48000, 44100};
    int i, r;
    double t_activate[3][90], t_mix[3][90], t_fmt[3][90], t_init[3][90];
    double t_start[3][90], t_stop[3][90], t_total[3][90];
    int n[3] = {0, 0, 0};

    for (r = 0; r < 3; r++) {
        for (i = 0; i < iters; i++) {
            IAudioClient *ac = NULL;
            WAVEFORMATEX *mix = NULL;
            WAVEFORMATEXTENSIBLE wf;
            double a, b;

            a = qpc_s();
            if (FAILED(dev->Activate(kIID_IAudioClient, CLSCTX_ALL,
                                     NULL, (void **)&ac)) || !ac)
                break;
            b = qpc_s();
            t_activate[r][n[r]] = b - a;

            a = qpc_s();
            if (FAILED(ac->GetMixFormat(&mix))) { ac->Release(); break; }
            b = qpc_s();
            t_mix[r][n[r]] = b - a;

            make_fmt32(&wf, rates[r], 2);
            a = qpc_s();
            ac->IsFormatSupported(AUDCLNT_SHAREMODE_SHARED,
                                  (WAVEFORMATEX *)&wf, NULL);
            b = qpc_s();
            t_fmt[r][n[r]] = b - a;

            a = qpc_s();
            HRESULT hi = ac->Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM |
                AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                0, 0, (WAVEFORMATEX *)&wf, NULL);
            b = qpc_s();
            t_init[r][n[r]] = b - a;
            if (FAILED(hi)) { ac->Release(); break; }

            a = qpc_s();
            ac->Start();
            b = qpc_s();
            t_start[r][n[r]] = b - a;

            a = qpc_s();
            ac->Stop();
            b = qpc_s();
            t_stop[r][n[r]] = b - a;

            t_total[r][n[r]] = t_activate[r][n[r]] + t_mix[r][n[r]] +
                               t_fmt[r][n[r]] + t_init[r][n[r]] +
                               t_start[r][n[r]] + t_stop[r][n[r]];
            if (mix) { CoTaskMemFree(mix); mix = NULL; }
            ac->Release();
            n[r]++;
        }
    }

    fprintf(fr, "{\n  \"experiment\": \"e10-a0\",\n");
    fprintf(fr, "  \"section\": \"reopen_cost\",\n");
    fprintf(fr, "  \"method\": \"native Windows WASAPI; QPC-timed; each "
                "cycle = Activate+GetMixFormat+IsFormatSupported+"
                "Initialize(shared,AUTOCONVERTPCM|SRC_DEFAULT_QUALITY)+"
                "Start+Stop+Release; sequence 44.1k->48k->44.1k; "
                "iterations: %d per rate; silence only; no audible signal\",\n",
            iters);
    fprintf(fr, "  \"cycles\": [\n");
    for (r = 0; r < 3; r++) {
        double med_tot, min_tot, max_tot;
        double med_init, min_init, max_init;
        double *v;
        int k;
        /* median/min/max of total and init phases */
        {
            double *tmp = (double *)calloc(n[r], sizeof(double));
            if (!tmp) break;
            for (k = 0; k < n[r]; k++) tmp[k] = t_total[r][k];
            for (k = 1; k < n[r]; k++) {
                double x = tmp[k];
                int j = k - 1;
                while (j >= 0 && tmp[j] > x) { tmp[j + 1] = tmp[j]; j--; }
                tmp[j + 1] = x;
            }
            med_tot = tmp[n[r] / 2]; min_tot = tmp[0]; max_tot = tmp[n[r] - 1];
            free(tmp);
        }
        {
            double *tmp = (double *)calloc(n[r], sizeof(double));
            if (!tmp) break;
            for (k = 0; k < n[r]; k++) tmp[k] = t_init[r][k];
            for (k = 1; k < n[r]; k++) {
                double x = tmp[k];
                int j = k - 1;
                while (j >= 0 && tmp[j] > x) { tmp[j + 1] = tmp[j]; j--; }
                tmp[j + 1] = x;
            }
            med_init = tmp[n[r] / 2]; min_init = tmp[0]; max_init = tmp[n[r] - 1];
            free(tmp);
        }
        fprintf(fr, "%s  {\"rate\": %lu, \"iterations\": %d,\n",
                r ? ",\n" : "", (unsigned long)rates[r], n[r]);
        fprintf(fr, "    \"total_cycle_ms\": {\"median\": %.4f, \"min\": %.4f, "
                    "\"max\": %.4f},\n", med_tot * 1e3, min_tot * 1e3,
                max_tot * 1e3);
        fprintf(fr, "    \"activate_ms\": {\"median\": %.4f},"
                    "\"getmixformat_ms\": {\"median\": %.4f},"
                    "\"isformatsupported_ms\": {\"median\": %.4f},\n",
                arr_median(t_activate[r], n[r]) * 1e3,
                arr_median(t_mix[r], n[r]) * 1e3,
                arr_median(t_fmt[r], n[r]) * 1e3);
        fprintf(fr, "    \"initialize_ms\": {\"median\": %.4f, \"min\": %.4f, "
                    "\"max\": %.4f},\n", med_init * 1e3, min_init * 1e3,
                max_init * 1e3);
        fprintf(fr, "    \"start_ms\": {\"median\": %.4f},"
                    "\"stop_ms\": {\"median\": %.4f}}\n",
                arr_median(t_start[r], n[r]) * 1e3,
                arr_median(t_stop[r], n[r]) * 1e3);
        (void)v;
    }
    fprintf(fr, "  ]\n}\n");
}

/* ---------------------------------------------------------------- */
/* main                                                              */
/* ---------------------------------------------------------------- */

int main(int argc, char **argv) {
    const char *outdir;
    char path[1024];
    FILE *fe = NULL, *ff = NULL, *fr = NULL;
    IMMDeviceEnumerator *enumr = NULL;
    IMMDeviceCollection *coll = NULL;
    IMMDevice *defdev = NULL;
    wchar_t *defid = NULL;
    UINT n = 0, i;
    HRESULT hr;

    if (argc < 2) {
        fprintf(stderr, "usage: qn_wasapi_probe <outdir>\n");
        return 2;
    }
    outdir = argv[1];

    if (FAILED(CoInitializeEx(NULL, COINIT_MULTITHREADED))) return 1;

    snprintf(path, sizeof(path), "%s\\a0-windows-endpoints.json", outdir);
    fe = fopen(path, "w");
    snprintf(path, sizeof(path), "%s\\a0-format-support.json", outdir);
    ff = fopen(path, "w");
    snprintf(path, sizeof(path), "%s\\a0-reopen.json", outdir);
    fr = fopen(path, "w");
    if (!fe || !ff || !fr) { fprintf(stderr, "cannot open outputs\n"); return 1; }

    hr = CoCreateInstance(kCLSID_MMDeviceEnumerator, NULL, CLSCTX_ALL,
                          kIID_IMMDeviceEnumerator, (void **)&enumr);
    if (FAILED(hr)) {
        fprintf(fe, "{\"error\": \"IMMDeviceEnumerator: %s\"}\n",
                hres_name(hr));
        return 1;
    }

    /* default render endpoint */
    hr = enumr->GetDefaultAudioEndpoint(eRender, eConsole, &defdev);
    if (SUCCEEDED(hr) && defdev) {
        defdev->GetId(&defid);
        defdev->Release();
    }

    hr = enumr->EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE, &coll);
    if (FAILED(hr)) {
        fprintf(fe, "{\"error\": \"EnumAudioEndpoints: %s\"}\n",
                hres_name(hr));
        return 1;
    }
    coll->GetCount(&n);

    fprintf(fe, "{\n  \"experiment\": \"e10-a0\",\n");
    fprintf(fe, "  \"section\": \"windows_render_endpoints\",\n");
    fprintf(fe, "  \"note\": \"native Windows enumeration via WASAPI; "
                "endpoint ids hashed (FNV-1a) for privacy; friendly names "
                "kept as device model only\",\n");
    fprintf(fe, "  \"endpoints\": [\n");
    for (i = 0; i < n; i++) {
        IMMDevice *dev = NULL;
        wchar_t *id = NULL;
        IPropertyStore *ps = NULL;
        PROPVARIANT pv;
        char idhash[41] = "";
        int is_def = 0;

        if (FAILED(coll->Item(i, &dev))) continue;
        dev->GetId(&id);
        if (id && defid && wcscmp(id, defid) == 0) is_def = 1;
        if (id) hex_sha1(id, wcslen(id) * sizeof(wchar_t), idhash);

        if (SUCCEEDED(dev->OpenPropertyStore(STGM_READ, &ps))) {
            PropVariantInit(&pv);
            ps->GetValue(PKEY_Device_FriendlyName, &pv);
            fprintf(fe, "%s  {\"endpoint_id_hash\": \"%s\", \"is_default\": %s, "
                        "\"friendly_name\": ",
                    i ? ",\n" : "", idhash, is_def ? "true" : "false");
            if (pv.vt == VT_LPWSTR) json_escape(fe, pv.pwszVal);
            else fprintf(fe, "null");
            fprintf(fe, "}\n");
            PropVariantClear(&pv);
            ps->Release();
        }
        if (id) CoTaskMemFree(id);
        dev->Release();
    }
    fprintf(fe, "  ]\n}\n");

    fprintf(ff, "{\n  \"experiment\": \"e10-a0\",\n");
    fprintf(ff, "  \"section\": \"format_support\",\n");
    fprintf(ff, "  \"method\": \"IsFormatSupported(shared/exclusive) and "
                "Initialize paths for Float32 interleaved stereo at each "
                "rate; silence only; format acceptance is NOT bit-perfect "
                "evidence\",\n");
    fprintf(ff, "  \"rows\": [\n");
    {
        int first = 1;
        for (i = 0; i < n; i++) {
            IMMDevice *dev = NULL;
            wchar_t *id = NULL;
            endpoint_t ep;
            char idhash[41] = "";
            if (FAILED(coll->Item(i, &dev))) continue;
            dev->GetId(&id);
            memset(&ep, 0, sizeof(ep));
            ep.dev = dev;
            ep.id = id;
            if (id) hex_sha1(id, wcslen(id) * sizeof(wchar_t), idhash);
            if (!first) fprintf(ff, ",\n");
            fprintf(ff, "  {\"endpoint_id_hash\": \"%s\",\n", idhash);
            fprintf(ff, "   \"results\": [\n");
            probe_endpoint_formats(ff, &ep);
            fprintf(ff, "\n   ]}");
            if (id) CoTaskMemFree(id);
            dev->Release();
            first = 0;
        }
    }
    fprintf(ff, "\n  ]\n}\n");

    /* reopen cost on the default endpoint */
    if (defdev) {
        IMMDevice *dev = NULL;
        /* re-activate a fresh reference for reopen (defdev was released) */
        if (SUCCEEDED(enumr->GetDefaultAudioEndpoint(eRender, eConsole,
                                                     &dev))) {
            run_reopen(fr, dev);
            dev->Release();
        }
    } else {
        fprintf(fr, "{\"error\": \"no default render endpoint\"}\n");
    }

    if (defid) CoTaskMemFree(defid);
    if (coll) coll->Release();
    if (enumr) enumr->Release();
    CoUninitialize();
    fclose(fe);
    fclose(ff);
    fclose(fr);
    return 0;
}
