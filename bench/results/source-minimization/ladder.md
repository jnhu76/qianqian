| Stage | TU | `.a` bytes | linked raw | **linked stripped** | stripped+xz | Corpus | MP3 xRT (min) | FLAC xRT |
|---|---:|---:|---:|---:|---:|---|---:|---:|
| s0 | 205 | 2,658,174 | 973,048 | 973,048 | 350,604 | PASS | 956 | 964 |
| s3-pthreads | 106 | 1,535,360 | 952,568 | 952,568 | 342,644 | PASS | 952 | 962 |
| s4-full-gc | 205 | 3,113,446 | 739,568 | 739,568 | 263,940 | PASS | 973 | 925 |
| s4-gc | 106 | 1,801,016 | 719,080 | 719,080 | 255,868 | PASS | 962 | 941 |
| s5-Os | 106 | 1,196,120 | 690,504 | 690,504 | 241,468 | PASS | 779 | 877 |
| s5-Os-LTO | 106 | 3,927,032 | 530,664 | 530,664 | 180,408 | PASS | 762 | 876 |
| s6-shipped-so (.so) | — | — | 1,019,536 | 949,016 | 339,984 | PIC PASS + smoke | — | — |

Linked-size decomposition (stripped): GC 233,480 + source closure 20,488 + codegen 188,416 = 442,384 B total.
