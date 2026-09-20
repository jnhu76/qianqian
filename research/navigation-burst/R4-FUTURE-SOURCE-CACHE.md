# R4 — FUTURE SOURCE / CACHE (forward-compatibility study only)

Campaign: QIANQIAN-NAVIGATION-BURST-ROOT-CAUSE-AND-BOUNDARY-0. Short by
design (§44). Nothing here is implemented; no networking refactor
touches the current local player.

## How local/remote sources fit later

```text
MediaRef (path today; URL/NAS later)
    ↓
MediaSource (owns bytes access + lifecycle)
    ↓
Seekable ByteSource (HTTP Range / file; chunked)
    ↓
Decode Plugin (UNCHANGED — SongCore never learns where bytes came from)
    ↓
DecodedPcmStream → PcmEdge → Output   (unchanged data plane)
```

What already approximates this: the decode provider boundary
(`PcmDecode::open_media(path)` → `DecodedPcmStream`, ports.rs) is the
seam a MediaSource would sit behind; `probe_media` (SourceFacts) is
already source-agnostic. What would change LATER: the `PathBuf`
constructor argument becomes a MediaRef, and one ByteSource abstraction
feeds the decoder. What must NOT change now: everything (the local
player is correct; PathBuf stays until a second source kind exists —
AGENTS razor).

## Where source-byte caching sits

Three levels, with cost reasoning:

```text
metadata/probe cache      tiny (SourceFacts per ref); cheap, safe
encoded source-byte cache PRIMARY durable cache
                          MP3/AAC/Opus: ~16–192 kbit/s ≈ 0.1–1.4 MB/min
decoded PCM               ~44.1 kHz × stereo × float32 ≈ 21 MB/min —
                          ~15× the 192 kbit/s top end, ~175× the
                          16 kbit/s low end of the encoded range above;
                          whole-song PCM caching is the WRONG default
                          (memory for what the decoder re-derives in ms)
short decode read-ahead   lives inside the source/decode path anyway
                          (today: edge 8192 frames + staging 1024)
```

Cache identity: `(source identity, version identity, byte range)` —
ETag / Last-Modified / content hash, NEVER URL only (URLs rotate;
content does not).

## Plugin vs resource (D13, to adjudicate WHEN network exists)

```text
MediaSource   PLUGIN only if it owns network client + auth + connection
              pool + retry workers with an independent failure domain —
              plausible, judged then
ContentCache  resource/service INSIDE the source owner if private;
              separately composed service only if several source kinds
              share one storage lifecycle/size budget/eviction policy
Prefetch      source-owned worker/resource first; never a Plugin by name
```

## Final-target intent controls prefetch (§36)

The burst architecture (R3) places interaction intent UPSTREAM of
expensive data-plane work — the same relation a future prefetch policy
needs: speculative low-priority prefetch of the LIKELY next track may
exist, but the moment a final target is named, it gets priority and
obsolete speculative work is cancelled/deprioritized. Intermediate
burst targets must NEVER trigger data-plane work (already true:
RC-A fix keeps them control-plane only).

## What must NOT be built now

Networking, cache, prefetch, MediaRef/ByteSource abstractions, PCM
caching, URL-keyed caches. The only durable obligation this campaign
adds to the future design: keep `probe`/`open` behind the existing
decode-provider boundary and keep navigation intent upstream of source
work — both already hold.
