# Release fixtures

Host-independent evidence objects for `tools/test_release.py`.

## `elf64-x86-64-minimal.o`

A real ELF64 x86-64 relocatable object (one exported symbol,
`qianqian_release_control`), committed so the `E14-real-ELF` control in
`tools/test_release.py` constructs a genuine non-PE archive member on any
host — including hosts whose `gcc` is MinGW (emits PE/COFF objects, never
ELF) or absent. The control packs this object into the synthetic
`build/artifacts/libsongcore.a` with `ar` and requires the `native` checker
to refuse it: every archive member must be `pe-x86-64` (`tools/check_release.py`,
`native_identity`, R3 "native archive target mismatch"). The fixture is
executable evidence, not authority: it exercises the real objdump-based R3
format gate against real ELF bytes; it makes no claim about any production
build output.

Self-verifying: if this file were replaced by anything `objdump` reads as
`pe-x86-64` — or cannot read at all — the control fails loudly, because the
harness asserts exit 2 with `POLICY_RED: R3 native archive target mismatch`.
A vacuous pass is not reachable.

Identity:

```text
sha256  b2157d65adc354c1a7778d6a0f2663c0a26a5b27b22f90a308594cfe45f2a0a9
bytes   1256
format  ELF 64-bit LSB relocatable, x86-64, version 1 (SYSV), not stripped
symbol  qianqian_release_control
```

Producing toolchain (recorded at generation time). Regeneration with a
different toolchain yields different bytes; that is acceptable and does not
weaken the control, which depends on the objdump format verdict, not on this
exact hash:

```text
source   int qianqian_release_control(void) { return 7; }
command  gcc -c elf64-x86-64-minimal.c -o elf64-x86-64-minimal.o
compiler gcc (Ubuntu 15.2.0-16ubuntu1) 15.2.0, target x86_64-linux-gnu
host     Ubuntu 26.04.1 LTS under WSL2
         (kernel 6.18.54.1-microsoft-standard-WSL2)
date     2026-10-30
```
