# SongCore 0.2.0-rc.1 — linux-x86_64

SongCore is the local media -> PCM decode component of Qianqian: one local
media file in, one coherent song snapshot out (identity, metadata, artwork,
stream information, source-rate/source-layout Float32 interleaved PCM, seek,
EOF). The ABI authority is `include/songcore.h` alone (ABI v1, frozen);
packaging never modifies it.

**This container is a release-substrate verification archive, not a release.**
The version string is the one the packaging caller supplied. The concrete next
release number is owner-reserved (contract section 14 Q1); packaging creates no
tag, no GitHub Release and no other published asset.

Support is claimed for `linux-x86_64` only. No other platform, architecture,
distro family or runtime ABI beyond ABI v1 is claimed by this package.

## Contents

    include/songcore.h                     the ONLY ABI description (ABI v1, frozen)
    lib/libsongcore.a                      static artifact (FFmpeg closure merged in,
                                           280 elf64-x86-64 members; static-first policy)
    lib/shared/libsongcore.so              optional shared secondary (same closure)
    LICENSES/                              FFmpeg COPYING set (extracted from the
                                           sha256-verified pinned n9.0.1 source zip),
                                           THIRD_PARTY_NOTICES.txt, MIT + Apache-2.0
                                           component texts (repo-root, B4)
    metadata/manifest.json                 provenance, schema_version 2
    metadata/symbols.txt                   the 15 ABI v1 exports
    metadata/checksums.txt                 sha256 of every packaged file except itself
    metadata/external-consumer-example.c   consumer smoke (open->probe->decode->seek->EOF->close)

## Verify the package

    sha256sum -c metadata/checksums.txt    # from inside this package root

## Link against it (C consumer link contract, contract section 9)

Static (the supported consumption path):

    gcc -O2 -Iinclude your.c -Llib -lsongcore -lm -lpthread -o your

    libc (glibc), libm and libpthread are the consumer link-time
    requirements; the FFmpeg closure ships in object form inside the archive,
    so no standalone FFmpeg runtime library is needed or shipped.

Shared (secondary):

    gcc -O2 -Iinclude your.c -Llib/shared -lsongcore -lm -lpthread \
        -Wl,-rpath,'$ORIGIN' -o your

    NEEDED of lib/shared/libsongcore.so: libm.so.6 libc.so.6
    glibc symbol-version floor: GLIBC_2.43 (recorded in
    metadata/manifest.json runtime_dependencies.shared; which distro family is
    supported is an owner decision, contract section 14)

## Consumer smoke

    gcc -O2 -Iinclude metadata/external-consumer-example.c -Llib -lsongcore -lm -lpthread -o songcore-consumer-example
    ./songcore-consumer-example <media-file>

## What packing verified about these bytes

Packing audited, with GNU binutils, that `include/songcore.h` is byte-identical
to the source commit, that the archive is an ar archive whose 280 members
are all elf64-x86-64, that both forms expose exactly the 15 ABI v1 entry points
declared by the header, and that neither form depends on an FFmpeg runtime
library. Those are build-time facts about the packaged bytes. They are not a
runtime, device or distro test and they do not establish platform support
beyond the audit above.

## Relink obligations (LGPL-2.1-or-later, contract section 10 O3)

The archive contains the FFmpeg members in object form (effective license
LGPL-2.1-or-later: the codec-base profile keeps gpl/version3/nonfree off and
pulls no external lib* copylibs). The machine-verifiable rebuild inputs --
native/ffmpeg/pin.json (tag->commit + zip sha256), the codec-base capability
profile, the linux-x86_64 target recipe, the imported closure manifest
(schema 2, units + configure args) and the Xmake replay configuration -- live
in the source repository at commit 69ba1ce05a2772f5aa077c6ece3365c40f6e0500; a licensee can recombine them to
relink a modified FFmpeg closure without any undocumented step.
