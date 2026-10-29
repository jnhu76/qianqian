# SongCore 0.2.0-rc.1 -- windows-x86_64

SongCore is the local media -> PCM decode component of Qianqian: one local
media file in, one coherent song snapshot out (identity, metadata, artwork,
stream information, source-rate/source-layout Float32 interleaved PCM, seek,
EOF). The ABI authority is include/songcore.h alone (ABI v1, frozen);
packaging never modifies it.

This is a release-candidate verification archive (#172), NOT a release: the
concrete version number is provisional -- the next release tag is
owner-reserved (contract section 14 Q1). songcore-v0.1.0 remains the
immutable historical baseline.

## Contents

    include/songcore.h                  the ONLY ABI description (ABI v1, frozen)
    lib/libsongcore.a                   static artifact (FFmpeg closure merged in,
                                        280 pe-x86-64 members, static-first policy)
    LICENSES/                           FFmpeg COPYING set (extracted from the
                                        sha256-verified pinned n9.0.1 source zip),
                                        THIRD_PARTY_NOTICES.txt (published
                                        songcore-v0.1.0 asset), MIT + Apache-2.0
                                        component texts (repo-root, B4)
    metadata/manifest.json              provenance, schema_version 2
    metadata/symbols.txt                the 15 ABI v1 exports
    metadata/checksums.txt              sha256 of every packaged file except itself
    metadata/external-consumer-example.c  consumer smoke (open->probe->decode->seek->EOF->close)

## Verify the package

    sha256sum -c metadata/checksums.txt   # from inside the package root

## Link against it (C consumer link contract)

Static (the supported consumption path; contract section 9):

    gcc -O2 -Iinclude your.c -Llib -lsongcore -lbcrypt -o your.exe

The explicit link list is exactly: -lsongcore -lbcrypt.

    -lsongcore   the merged-closure static archive (FFmpeg ships in object
                 form inside it; no standalone FFmpeg runtime library is
                 needed or shipped)
    -lbcrypt     REQUIRED: the archive's Win32 entropy path imports
                 BCryptOpenAlgorithmProvider / BCryptGenRandom /
                 BCryptCloseAlgorithmProvider (audited per contract section 9;
                 recorded in metadata/manifest.json runtime_dependencies)

The mingw-w64 runtime (msvcrt-family CRT + Win32 system imports) is provided
by the toolchain that links the consumer; system-DLL allowlist discipline is
codified in tools/check_release.py (SYSTEM_DLLS incl. bcrypt,
bcryptprimitives, msvcrt).

## Consumer smoke

    gcc -O2 -Iinclude metadata/external-consumer-example.c -Llib -lsongcore -lbcrypt -o songcore-consumer-example.exe
    ./songcore-consumer-example.exe <media-file>

## Relink obligations (LGPL-2.1-or-later, contract section 10 O3)

The archive contains the FFmpeg members in object form (effective license
LGPL-2.1-or-later: the codec-base profile keeps gpl/version3/nonfree off and
pulls no external lib* copylibs). The machine-verifiable rebuild inputs --
native/ffmpeg/pin.json (tag->commit + zip sha256), the codec-base capability
profile, the windows-x86_64 target recipe, the imported closure manifest
(schema 2, units + flags), and the Xmake replay configuration -- are part of
the source repository at the manifest's recorded commit; a licensee can
recombine them to relink a modified FFmpeg closure without any undocumented
step.