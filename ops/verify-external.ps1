# verify-external.ps1 — replayable EXTERNAL-consumer acceptance for the
# SongCore windows-x86_64 RC package (#172 S2-external).
#
# Simulates a user who knows nothing about any repository: given ONLY the RC
# zip, the script extracts it to a fresh %TEMP% directory and consumes it
# exclusively through package materials (include/, lib/, README link
# contract, metadata/). No repo-internal path is ever referenced by the
# compiled consumer; the repo is touched only as the source of the zip under
# test and of this script.
#
# Pipeline (fail-closed end to end; every child exit code propagated):
#   1. Zip identity: Get-FileHash(zip) must equal -ExpectedZipSha256.
#   2. Extract to %TEMP%\songcore-external-verify-<stamp> (recreated).
#   3. In-package integrity: re-hash every metadata/checksums.txt entry.
#   4. Write an independent C consumer (embedded below, written from
#      include/songcore.h alone) into the temp dir.
#   5. Media input: use -MediaFile, or generate a deterministic 10 s stereo
#      FLAC sine with ffmpeg from PATH (a user supplies their own media;
#      if none and no ffmpeg, this FAILS - never skipped).
#   6. Compile+link with the README link contract, verbatim:
#         gcc -O2 -Iinclude <consumer.c> -Llib -lsongcore -lbcrypt -o out.exe
#      (the README's declared link list is exactly -lsongcore -lbcrypt).
#   7. Run open->probe->decode->seek(50%)->EOF->close; require exit 0 and
#      the consumer's PASS marker.
#   8. Runtime dependency audit: objdump -p on the exe; every "DLL Name:"
#      must be inside the README-declared allowlist
#      {bcrypt.dll, kernel32.dll, msvcrt.dll}. Anything else fails.
#   9. Emit one JSON summary line on stdout; detailed log in
#      evidence/songcore-172/external-accept/<stamp>/external-log.txt.
#
# Prerequisites (machine-level, not repo-level): a mingw-w64 gcc with
# binutils (default probe: -GccDir, then PATH), ffmpeg for media generation.
#
# Identity note: the S2 ask pinned SHA256 b2c222cd4efa6b0e8f4fc090181a0cf7e
# b7beb9b35c4f5b2ddf07733f223aa7, but per verdicts/s2-package.md §6 that is
# the FIRST container, deliberately voided and not retained ("作废，不另留
# 副本"). The current sidecar-attested container (SHA256SUMS +
# package-summary.json, rebuilt 2026-10-29 07:05) is the default below.

[CmdletBinding()]
param(
    [string]$ZipPath = "C:\Users\fred1\source\qianqian-172\evidence\songcore-172\package\songcore-v0.2.0-rc.1-windows-x86_64.zip",
    [string]$ExpectedZipSha256 = "7b4d0615d25af464739373a0a3565f77f3208ce9c263f59e578d443801536415",
    [string]$GccDir = "C:\Users\fred1\tools\mingw64\bin",
    [string]$MediaFile = "",
    [string]$LogRoot = "C:\Users\fred1\source\qianqian-172\evidence\songcore-172\external-accept"
)

$ErrorActionPreference = "Stop"
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$tempRoot = Join-Path $env:TEMP "songcore-external-verify-$stamp"
$failures = New-Object System.Collections.Generic.List[string]

function Fail([string]$stage, [string]$msg) {
    Write-Host "FAIL [$stage] $msg"
    $script:failures.Add("[$stage] $msg")
    throw "fail-closed: $stage"
}

if (-not (Test-Path $ZipPath)) { Fail "preflight" "zip not found: $ZipPath" }

# --- 1. zip identity ---
$zipHash = (Get-FileHash -Algorithm SHA256 $ZipPath).Hash.ToLowerInvariant()
Write-Host "zip sha256: $zipHash"
if ($zipHash -ne $ExpectedZipSha256.ToLowerInvariant()) {
    Fail "zip-identity" "sha256 mismatch: got $zipHash expected $ExpectedZipSha256"
}

if (-not (Test-Path $LogRoot)) { New-Item -ItemType Directory -Path $LogRoot -Force | Out-Null }
$logPath = Join-Path $LogRoot "$stamp-external-log.txt"
Start-Transcript -Path $logPath | Out-Null
try {
    # --- 2. extract ---
    if (Test-Path $tempRoot) { Remove-Item -Recurse -Force $tempRoot }
    New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
    Expand-Archive -Path $ZipPath -DestinationPath $tempRoot -Force
    $pkg = Get-ChildItem -Directory $tempRoot | Select-Object -First 1
    if (-not $pkg) { Fail "extract" "no package root directory after extraction" }
    Write-Host "extracted to: $tempRoot (root: $($pkg.Name))"

    # --- 3. in-package checksums ---
    $checksums = Join-Path $pkg.FullName "metadata\checksums.txt"
    if (-not (Test-Path $checksums)) { Fail "integrity" "metadata/checksums.txt missing" }
    $nOk = 0
    foreach ($line in Get-Content $checksums) {
        if ($line -match "^\s*$" -or $line -match "^#") { continue }
        $parts = $line -split "\s+", 2
        if ($parts.Count -ne 2) { Fail "integrity" "malformed checksums.txt line: $line" }
        $rel = $parts[1].Trim().TrimStart("*")
        $file = Join-Path $pkg.FullName ($rel -replace "/", "\")
        if (-not (Test-Path $file)) { Fail "integrity" "checksums.txt references missing file: $rel" }
        $h = (Get-FileHash -Algorithm SHA256 $file).Hash.ToLowerInvariant()
        if ($h -ne $parts[0].ToLowerInvariant()) {
            Fail "integrity" "hash mismatch for ${rel}: $h"
        }
        $nOk++
    }
    Write-Host "in-package checksums: $nOk entries OK"

    # --- 4. consumer source (embedded; uses ONLY include/ + lib/ + CRT) ---
    $consumerC = @'
/*
 * s2-external-consumer.c - independent external consumer acceptance (#172).
 * Written against include/songcore.h (ABI v1) only; knows nothing about any
 * repository. Sequence: open -> probe -> decode -> seek(50%) -> EOF -> close,
 * with return-value verification at every step. Exit 0 = PASS.
 */
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "songcore.h"

static int64_t host_read(void *ud, uint8_t *dst, size_t size) {
    size_t n = fread(dst, 1, size, (FILE *)ud);
    if (n > 0) return (int64_t)n;
    return feof((FILE *)ud) ? 0 : -1;
}
static int64_t host_seek(void *ud, int64_t absolute_offset) {
    if (fseek((FILE *)ud, (long)absolute_offset, SEEK_SET) != 0) return -1;
    return (int64_t)ftell((FILE *)ud);
}
static int64_t host_size(void *ud) {
    FILE *f = (FILE *)ud;
    long cur = ftell(f);
    if (fseek(f, 0, SEEK_END) != 0) return -1;
    long end = ftell(f);
    fseek(f, cur, SEEK_SET);
    return (int64_t)end;
}

static int fail(const char *stage, song_status st) {
    fprintf(stderr, "S2-CONSUMER FAIL at %s: status %d\n", stage, (int)st);
    return 1;
}

int main(int argc, char **argv) {
    if (argc != 2) { fprintf(stderr, "usage: %s <media-file>\n", argv[0]); return 2; }
    if (songcore_abi_version() != SONGCORE_ABI_VERSION) {
        fprintf(stderr, "S2-CONSUMER FAIL: abi version mismatch\n");
        return 1;
    }
    FILE *f = fopen(argv[1], "rb");
    if (!f) { fprintf(stderr, "S2-CONSUMER FAIL: cannot open %s\n", argv[1]); return 2; }
    song_io io;
    io.userdata = f; io.read = host_read; io.seek = host_seek; io.size = host_size;
    song_handle *h = NULL;
    if (song_open(&io, &h) != SONG_OK || h == NULL) return fail("open", SONG_ERR_INTERNAL_ERROR);

    song_info info;
    memset(&info, 0, sizeof(info));
    if (song_probe(h, &info) != SONG_OK) return fail("probe", SONG_ERR_INTERNAL_ERROR);
    if (info.sample_rate <= 0 || info.channels <= 0 || info.duration_us <= 0 ||
        info.codec[0] == 0 || info.container[0] == 0 || info.audio_stream_count < 1u) {
        fprintf(stderr, "S2-CONSUMER FAIL at probe: bad snapshot\n");
        return 1;
    }
    printf("probe: codec=%s container=%s rate=%d ch=%d duration_us=%" PRId64 "\n",
           info.codec, info.container, info.sample_rate, info.channels, info.duration_us);

    float *dst = (float *)malloc(sizeof(float) * 4096u * (unsigned)info.channels);
    if (!dst) return fail("alloc", SONG_ERR_OUT_OF_MEMORY);
    uint64_t total1 = 0;
    for (;;) {
        uint64_t got = 0;
        song_status st = song_read_pcm(h, dst, 4096, &got);
        if (st == SONG_EOF) { if (got != 0) return fail("decode_eof_shape", st); break; }
        if (st != SONG_OK) return fail("decode", st);
        if (got == 0 || got > 4096) return fail("decode_shape", SONG_ERR_INTERNAL_ERROR);
        total1 += got;
    }
    if (total1 == 0) return fail("decode_empty", SONG_ERR_INTERNAL_ERROR);
    printf("decode: %" PRIu64 " frames to SONG_EOF\n", total1);

    int64_t landing = -2;
    if (song_seek(h, info.duration_us / 2, &landing) != SONG_OK) return fail("seek", SONG_ERR_INTERNAL_ERROR);
    printf("seek: target_us=%" PRId64 " landing_us=%" PRId64 "\n", info.duration_us / 2, landing);

    uint64_t got = 0;
    if (song_read_pcm(h, dst, 4096, &got) != SONG_OK || got == 0)
        return fail("post_seek_read", SONG_ERR_INTERNAL_ERROR);
    uint64_t total2 = got;
    for (;;) {
        uint64_t g2 = 0;
        song_status st = song_read_pcm(h, dst, 4096, &g2);
        if (st == SONG_EOF) { if (g2 != 0) return fail("eof_shape", st); break; }
        if (st != SONG_OK) return fail("post_seek_decode", st);
        if (g2 == 0 || g2 > 4096) return fail("post_seek_shape", SONG_ERR_INTERNAL_ERROR);
        total2 += g2;
    }
    if (total2 == 0) return fail("eof_empty", SONG_ERR_INTERNAL_ERROR);
    printf("eof: clean SONG_EOF after %" PRIu64 " post-seek frames\n", total2);

    song_close(h);
    song_close(NULL); /* documented no-op; must not crash */
    fclose(f);
    free(dst);
    printf("S2-CONSUMER PASS\n");
    return 0;
}
'@
    $consumerPath = Join-Path $tempRoot "s2-external-consumer.c"
    [System.IO.File]::WriteAllText($consumerPath, $consumerC)
    Write-Host "consumer source: $consumerPath"

    # --- 5. media input ---
    if ($MediaFile -ne "") {
        if (-not (Test-Path $MediaFile)) { Fail "media" "MediaFile not found: $MediaFile" }
        $media = (Resolve-Path $MediaFile).Path
    } else {
        $ffmpeg = Get-Command ffmpeg -ErrorAction SilentlyContinue
        if (-not $ffmpeg) { Fail "media" "no -MediaFile and ffmpeg not on PATH (a user supplies their own media; nothing to fail silently)" }
        $media = Join-Path $tempRoot "sample10s.flac"
        & $ffmpeg.Source -y -v error -f lavfi -i "sine=frequency=440:duration=10:sample_rate=44100" -af "aformat=sample_fmts=s16:channel_layouts=stereo" -c:a flac $media
        if ($LASTEXITCODE -ne 0) { Fail "media" "ffmpeg media generation failed (exit $LASTEXITCODE)" }
        if (-not (Test-Path $media)) { Fail "media" "ffmpeg produced no media file" }
    }
    Write-Host "media input: $media"

    # --- 6. compile+link per README contract ---
    $gcc = Join-Path $GccDir "gcc.exe"
    if (-not (Test-Path $gcc)) {
        $gcc = (Get-Command gcc -ErrorAction SilentlyContinue).Source
        if (-not $gcc) { Fail "toolchain" "gcc.exe not found in $GccDir nor on PATH" }
    }
    $exe = Join-Path $tempRoot "s2-external-consumer.exe"
    $gccArgs = @("-O2", "-Iinclude", $consumerPath, "-Llib", "-lsongcore", "-lbcrypt", "-o", $exe)
    Write-Host "link contract: gcc $($gccArgs -join ' ')"
    Push-Location $pkg.FullName
    try {
        & $gcc @gccArgs
        if ($LASTEXITCODE -ne 0) { Fail "compile" "gcc exit $LASTEXITCODE with the README link contract" }
    } finally { Pop-Location }
    if (-not (Test-Path $exe)) { Fail "compile" "exe not produced" }
    Write-Host "compile+link: OK (exact README link list -lsongcore -lbcrypt)"

    # --- 7. run the sequence ---
    $out = & $exe $media
    $exitCode = $LASTEXITCODE
    $out | ForEach-Object { Write-Host "  consumer: $_" }
    if ($exitCode -ne 0) { Fail "run" "consumer exit $exitCode" }
    $joined = ($out -join "`n")
    if ($joined -notmatch "S2-CONSUMER PASS") { Fail "run" "consumer output lacks PASS marker" }
    Write-Host "sequence open->probe->decode->seek(50%)->EOF->close: PASS (exit 0)"

    # --- 8. runtime dependency audit ---
    $objdump = Join-Path $GccDir "objdump.exe"
    if (-not (Test-Path $objdump)) {
        $objdump = (Get-Command objdump -ErrorAction SilentlyContinue).Source
        if (-not $objdump) { Fail "toolchain" "objdump.exe not found in $GccDir nor on PATH" }
    }
    $imports = & $objdump -p $exe | Select-String "DLL Name:" | ForEach-Object {
        ($_.Line -split ":", 2)[1].Trim()
    }
    if (-not $imports -or $imports.Count -eq 0) { Fail "imports" "no DLL imports parsed from exe" }
    $allow = @("bcrypt.dll", "kernel32.dll", "msvcrt.dll")
    $undeclared = @($imports | Where-Object { $allow -notcontains $_.ToLowerInvariant() })
    Write-Host ("import table: " + ($imports -join ", "))
    if ($undeclared.Count -gt 0) {
        Fail "imports" ("undeclared DLL imports outside the README allowlist: " + ($undeclared -join ", "))
    }
    Write-Host "runtime deps: all imports inside README-declared allowlist (bcrypt declared; kernel32/msvcrt = toolchain CRT + Win32 system imports)"

    Write-Host ""
    Write-Host "EXTERNAL-CONSUMER ACCEPTANCE: PASS"
    $verdict = "PASS"
} finally {
    Stop-Transcript | Out-Null
}

$summary = [ordered]@{
    check       = "songcore-172 s2-external external-consumer acceptance"
    zip         = $ZipPath
    zip_sha256  = $zipHash
    extract_dir = $tempRoot
    media       = $media
    sequence    = "open=OK probe=OK decode=OK seek50=OK eof=OK close=OK (exit 0)"
    imports     = @($imports)
    verdict     = $verdict
    log         = $logPath
}
$summary | ConvertTo-Json -Depth 4 -Compress
exit 0
