# E05 — KiteCodec Reference Audit

## Purpose

不 fork，不假设采用。

只回答：

> KiteCodec 哪些已经解决的工程问题值得直接借鉴？

## Audit Scope

重点查看：

- KMP cinterop；
- JVM/Android JNI；
- FFmpeg binary packaging；
- target matrix；
- static linking；
- resource extraction；
- decoder ownership；
- seek implementation；
- error translation；
- license packaging。

## Explicitly do NOT inherit by default

- video pipeline；
- encoder；
- muxer；
- filter graph；
- transcoder；
- generic media abstraction；
- dav1d；
- video surface；
- broad FFmpeg API exposure。

## Deliverable

输出一张表：

| Concern | Reuse idea | Reject | Reason |
|---|---|---|---|

原则：

> implementation quarry, not architecture authority.
