#!/usr/bin/env python3
"""Lane C (windows) — emit stage.json with real argv/exit codes and the
sha256 of every evidence file in the lane directory."""
import hashlib
import json
from pathlib import Path

LANE = Path("/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/lane-c-windows-pe")
PKG = "/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/win/extract/songcore-v0.0.0-preflight-windows-x86_64"
RUNW = r"C:\Users\fred1\lane-c-win-pe"

VERDICT = "PASS"

argv = [
    # --- WSL build host: toolchain + input identity ---
    "x86_64-w64-mingw32-gcc --version",
    "x86_64-w64-mingw32-gcc -dumpmachine",
    "x86_64-w64-mingw32-ar --version",
    "x86_64-w64-mingw32-objdump --version",
    "uname -a",
    "git -C /mnt/c/Users/fred1/source/qianqian rev-parse HEAD",
    f"sha256sum {PKG}/include/songcore.h {PKG}/lib/libsongcore.a",
    f"x86_64-w64-mingw32-nm -A -u {PKG}/lib/libsongcore.a | grep -i bcrypt",
    "diff -r /home/hoo/g4-172/laneB-stage/windows/songcore-v0.0.0-preflight-windows-x86_64 " + PKG,
    # --- negative control, then the contract section 9 link line ---
    f"x86_64-w64-mingw32-gcc -O2 -I{PKG}/include pe-consumer.c -L{PKG}/lib -lsongcore -o pe-consumer-nobcrypt.exe   # negative control, -lbcrypt omitted",
    f"x86_64-w64-mingw32-gcc -O2 -I{PKG}/include pe-consumer.c -L{PKG}/lib -lsongcore -lbcrypt -o $HOME/lane-c-pe/out/pe-consumer.exe",
    "file $HOME/lane-c-pe/out/pe-consumer.exe",
    "x86_64-w64-mingw32-objdump -f $HOME/lane-c-pe/out/pe-consumer.exe",
    f"x86_64-w64-mingw32-gcc -O2 -I{PKG}/include -c pe-consumer.c -o pe-consumer.o",
    "x86_64-w64-mingw32-nm -u pe-consumer.o | grep -i bcrypt   # none: consumer source has no BCrypt reference",
    "x86_64-w64-mingw32-objdump -p $HOME/lane-c-pe/out/pe-consumer.exe   # import table walk",
    # --- stage into the isolated Windows-visible directory ---
    "cp -f $HOME/lane-c-pe/out/pe-consumer.exe /mnt/c/Users/fred1/lane-c-win-pe/pe-consumer.exe",
    "cp -f native/experiments/songcore-equivalence/fixtures/{mp3-cbr-id3v23.mp3,flac-16-44-stereo.flac,alac-16-44-stereo.m4a,alac-long.m4a} /mnt/c/Users/fred1/lane-c-win-pe/media/",
    "sha256sum /mnt/c/Users/fred1/lane-c-win-pe/pe-consumer.exe /mnt/c/Users/fred1/lane-c-win-pe/media/*",
    "cmp $HOME/lane-c-pe/out/pe-consumer.exe /mnt/c/Users/fred1/lane-c-win-pe/pe-consumer.exe",
    "cmp <each committed fixture> /mnt/c/Users/fred1/lane-c-win-pe/media/<same name>",
    # --- execution on the real Windows host (Git Bash = native Windows process) ---
    r"C:\Users\fred1\lane-c-win-pe\pe-consumer.exe --selftest",
    r"C:\Users\fred1\lane-c-win-pe\pe-consumer.exe media\mp3-cbr-id3v23.mp3",
    r"C:\Users\fred1\lane-c-win-pe\pe-consumer.exe media\flac-16-44-stereo.flac",
    r"C:\Users\fred1\lane-c-win-pe\pe-consumer.exe media\alac-16-44-stereo.m4a",
    r"C:\Users\fred1\lane-c-win-pe\pe-consumer.exe media\alac-long.m4a",
    r"cmd.exe /c C:\Users\fred1\lane-c-win-pe\pe-consumer.exe C:\Users\fred1\lane-c-win-pe\media\mp3-cbr-id3v23.mp3",
    r"C:\Users\fred1\lane-c-win-pe\pe-consumer.exe --sha256-file media\mp3-cbr-id3v23.mp3",
    r"C:\Users\fred1\lane-c-win-pe\pe-consumer.exe   (no arguments: usage exit class)",
    # --- reference comparison ---
    "python3 build/compare_reference.py   # PCM sha256 vs native/experiments/songcore-equivalence/reference.json",
]

exit_codes = [
    0, 0, 0, 0, 0, 0,          # toolchain identity
    0,                          # sha256sum package inputs
    0,                          # nm archive: three BCrypt* undefined in random_seed.c.obj
    0,                          # diff -r WSL staged view == build input
    1,                          # negative control: link WITHOUT -lbcrypt fails (expected)
    0,                          # link WITH -lbcrypt succeeds
    0, 0,                       # file / objdump -f
    0,                          # compile consumer.o
    1,                          # nm -u consumer.o | grep bcrypt -> no match (expected)
    0,                          # objdump -p import table
    0,                          # cp exe
    0,                          # cp fixtures
    0,                          # sha256sum staged tree
    0,                          # cmp exe
    0,                          # cmp fixtures
    0,                          # --selftest (SHA256_SELFTEST=PASS on Windows)
    0,                          # mp3 full walk
    0,                          # flac full walk
    0,                          # alac-16-44 full walk
    0,                          # alac-long full walk
    0,                          # cmd.exe driven mp3 full walk
    0,                          # --sha256-file cross-check
    2,                          # no-args usage (expected non-zero usage class)
    0,                          # reference comparison: COMPARE_RESULT=PASS
]

def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()

skip = {"stage.json", "build/sha256-manifest.txt"}
files = sorted(p for p in LANE.rglob("*") if p.is_file() and str(p.relative_to(LANE)).replace("\\", "/") not in skip)
paths = [str(p) for p in files]
hashes = [sha(p) for p in files]

stage = {
    "stage_id": "lane-c-windows-pe-20261030",
    "source_sha": "69ba1ce05a2772f5aa077c6ece3365c40f6e0500",
    "source_sha_note": "git -C the Windows workspace and git -C the WSL repo both report 69ba1ce05a2772f5aa077c6ece3365c40f6e0500; tracked tree clean (only untracked evidence/ paths)",
    "input_sha": {
        "package_tree": PKG,
        "package_tree_note": "14 files; identical byte-for-byte to the WSL staged view /home/hoo/g4-172/laneB-stage/windows/songcore-v0.0.0-preflight-windows-x86_64 (diff -r exit 0)",
        "include/songcore.h": "cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536",
        "lib/libsongcore.a": "89d0428db2152da60f233290c082ecfb33b5590628ab9c40b97e8354afc65e97",
        "metadata/manifest.json": "660bb3dc86e2a3e8079b95b61d0b08b80d4248ac5cac15799b27af7def9f1bfa",
        "metadata/checksums.txt": "ae3f3382ae33c24fb55b9ccf8aa06ce91a9989ece3b6abd0c507deeb42ec8470",
        "shared_form_absent": "no lib/shared/ directory, no DLL/.so anywhere in the package (find exit 0, zero hits); manifest artifact_shared=false, shared_sha256=null",
        "media_fixtures_staged": {
            "mp3-cbr-id3v23.mp3": "feb57f60b7e1dfe9b233152b531e3df7e36cf645699fd1c2f86062cc8fc4c058",
            "flac-16-44-stereo.flac": "d80626686047762d2b3334ca27536799524b42d0e89aa004bd1bbe776551a4a2",
            "alac-16-44-stereo.m4a": "0ac59af02a8f80d309ce84050adc07c8180b0886be25f975672fa46501c448e3",
            "alac-long.m4a": "2d0bbae102dbdfd6236c3e43093303998df75872f5c6928f98649179fa00d52c"
        },
        "media_fixtures_note": "each staged copy is cmp-identical to the committed fixture under native/experiments/songcore-equivalence/fixtures/ and equals reference.json fixture_sha256",
    },
    "target_toolchain": {
        "target": "windows-x86_64 (x86_64-pc-windows-gnu, mingw32 target_os)",
        "build_host": "WSL2 Ubuntu-24.04, Linux 6.18.54.1-microsoft-standard-WSL2",
        "compiler": "x86_64-w64-mingw32-gcc (GCC) 13-win32",
        "binutils": "GNU ar / objdump / nm (GNU Binutils) 2.45.90.20260125",
        "link_line": "x86_64-w64-mingw32-gcc -O2 -I<Pkg>/include pe-consumer.c -L<Pkg>/lib -lsongcore -lbcrypt -o pe-consumer.exe   (contract section 9 windows static line; -lbcrypt present, no other system -l)",
        "artifact": "PE32+ executable for MS Windows 5.02 (console), x86-64, 18 sections (objdump -f: pei-x86-64)",
        "consumer_exe_sha256": "3ec3c672bd8d2f0d0d92604ca1a1d07a76b479c2c2e306f6b3ef09eca4cbc60d",
        "consumer_exe_path_executed": RUNW + r"\pe-consumer.exe",
        "run_host": "Windows 11 专业版, version 10.0.26300 build 26300 (cmd /c ver: Microsoft Windows [版本 10.0.26300.9818]); runner Git Bash MINGW64_NT-10.0-26300, x86_64 Msys",
    },
    "argv": argv,
    "exit_codes": exit_codes,
    "evidence_paths": paths,
    "evidence_sha256": hashes,
    "verdict": VERDICT,
    "verdict_basis": [
        "PE consumer built in WSL against the packaged include/songcore.h + lib/libsongcore.a and executed on the real Windows host; exit 0 on all four reference fixtures plus the cmd.exe-driven repeat",
        "full-decode PCM sha256 equals reference.json for mp3-cbr-id3v23, flac-16-44-stereo, alac-16-44-stereo and alac-long (frames, rate, channels, codec and clean SONG_EOF terminal also match)",
        "seek -> landing -> post-seek decode -> clean EOF executed on all four fixtures (status 0, non-zero post-seek frames)",
        "-lbcrypt is genuinely required: omitting it fails the link with undefined BCrypt{OpenAlgorithmProvider,GenRandom,CloseAlgorithmProvider} from libsongcore.a(random_seed.c.obj); with it the link exits 0",
        "the produced PE imports exactly the three contract section 9 BCrypt names from bcrypt.dll, and the consumer object itself has no BCrypt reference, so the dependency comes from the shipped archive",
        "bcrypt runtime: loader bound bcrypt.dll (process started, exit 0); the consumer replayed FFmpeg's exact call sequence (BCryptOpenAlgorithmProvider(MS_PRIMITIVE_PROVIDER) -> BCryptGenRandom(h,buf,len,0) -> BCryptCloseAlgorithmProvider) with all statuses 0x0",
        "the IAT counter instrument is proven live by its own positive control (delta=1 PASS); with it, the archive made 0 BCryptGenRandom calls from song_open through decode EOF and through close on every fixture, i.e. these fixtures do not trigger FFmpeg's av_get_random_seed() entropy path",
    ],
    "bcrypt_runtime_verification": {
        "what_is_proven": [
            "the shipped archive has a link-time requirement on bcrypt (negative-control link without -lbcrypt fails on exactly the three symbols in random_seed.c.obj; with -lbcrypt it links)",
            "the produced PE has a static import on bcrypt.dll for BCryptOpenAlgorithmProvider/BCryptGenRandom/BCryptCloseAlgorithmProvider (objdump -p)",
            "on this Windows host the loader resolved bcrypt.dll and the exe ran to completion (exit 0)",
            "the exact call shape used by FFmpeg n9.0.1 libavutil/random_seed.c:165-169 succeeds on this host (all NTSTATUS 0x0)",
            "the in-process IAT counter is functional (positive control moves it by exactly 1)",
        ],
        "what_is_not_proven": [
            "that SongCore/FFmpeg actually called BCryptGenRandom during these fixture runs: measured 0 calls open->decode-EOF and 0 through close, so the entropy path was NOT walked for mp3/flac/alac fixtures",
            "any audible/device behaviour: no Output Plugin or host backend is in this lane's scope",
        ],
    },
    "prior_archival_cross_reference": {
        "note": "earlier Windows gate receipts from the #329 experiments recorded the same reference PCM hashes with a different consumer source; this lane supersedes them for the 69ba1ce packaging artifact",
        "evidence/songcore-172/q2-crossroot/gates/receipts/windows-win-r1/gate-windows-receipt.json": "a06111bb9f919f92b3991d8380effcbf4ea0e1b2a35c4ae52f49197183e40ea1 (consumer_source_sha256 edfed90c4e05a37d6e98f64b68817baac3f4f33086ffb471413573f221607fd6; pcm_sha256_match true for all four fixtures)",
        "evidence/songcore-172/q2-crossroot/gates/receipts/windows-windows-root-two/gate-windows-receipt.json": "18e1a04355ba4e66e47b20d94ae596d280612b3f74fe84fb7723ebaf1b0214db (same consumer_source_sha256; pcm_sha256_match true for all four fixtures)",
    },
    "not_run": [
        "shared/DLL consumer on windows: the package ships no shared form (contract section 8 static-only this round), so no .dll consumer link or run was possible",
        "MSVC / Windows-native toolchain build: the target recipe is the mingw-w64 cross toolchain only; no cl.exe/MSVC build was attempted",
        "FFmpeg av_get_random_seed() entropy path at runtime: instrumented (IAT counter + positive control) but measured 0 calls for all four fixtures, so that path was not exercised end-to-end",
        "multi-stream container / song_select_stream / song_audio_stream_info beyond index 0: all four fixtures are single-audio-stream files",
        "lifecycle negatives (mid-stream song_close, close(NULL), repeated close, read after close) and the ABI-invalid-argument matrix: owned by other lanes, not executed here",
        "32-bit (i686) and ARM64 windows targets, and any non-x86_64 Windows host",
        "audible/device output verification: no Output Plugin/host backend in scope for this lane",
        "Defender/AV or third-party DLL injection checks, and any long-run/memory-leak soak",
    ],
    "notes": [
        "the consumer's own SHA-256 is validated on the Windows host against three FIPS 180-4 vectors (--selftest) and against sha256sum for a 65 KB file (--sha256-file), so a hash match is not an artefact of a broken hasher",
        "a first build (sha256 49e8710e..., then b66cc6e1...) called BCryptGenRandom with BCRYPT_USE_SYSTEM_PREFERRED_RNG while passing a provider handle, which returns STATUS_INVALID_PARAMETER 0xC000000D; that was a defect in the verification probe, not in SongCore. It was corrected to FFmpeg's exact call shape, and the final artifact (3ec3c672...) is the only one whose results are reported. The pre-fix mp3 run is kept under raw/pre-fix/ and run/pre-fix-* for audit.",
        "host.txt was first captured by the runner with `cmd /c ver | grep`, which Git Bash treated as binary ('Binary file (standard input) matches'); it was regenerated with `grep -a` and powershell CIM fields. run/host.first-attempt.txt and run/host.runner-capture.txt preserve the earlier captures.",
        "this lane is a pre-flight verification of the windows-x86_64 candidate package; it does not declare #329 L1/L2/L3 RELEASE, creates no tag and no GitHub Release",
    ],
}

out = LANE / "stage.json"
out.write_text(json.dumps(stage, indent=2, ensure_ascii=False) + "\n")
print("wrote", out, "evidence files:", len(paths))
