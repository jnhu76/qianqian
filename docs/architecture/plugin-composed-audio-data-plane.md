# Plugin-composed, direct-flow Audio Data Plane

> **Status: PROPOSED architecture target.** Normative decision: [`../adr/ADR-PBK-002.md`](../adr/ADR-PBK-002.md). Until that ADR is ACCEPTED, registered ARCH-003/ARCH-004 authority remains unchanged.

## One-sentence model

> **The durable participants of the audio data plane are composed and lifetimed as Plugins/Fibers; PCM flows directly over pre-bound typed edges between those plugin instances, never through Composition Kernel dispatch per block.**

## Two orthogonal graphs

```text
COMPOSITION / LIFECYCLE GRAPH

              Composition Kernel
                      |
        +-------------+--------------+
        |             |              |
        v             v              v
     Decoder        Gain           Output
      Plugin        Plugin          Plugin
        |             |              |
     Fiber/lifecycle/capabilities/replacement


PCM PROCESSING GRAPH

Encoded -> Decoder -PCM-> SRC -PCM-> Gain -PCM-> EQ -PCM-> Output
                                   |
                                   +----PCM----> Analyzer
```

The same plugin instances may appear in both views, but the graphs mean different things:

```text
Composition graph
    who exists / who can reach whom / who can be replaced / who must outlive whom

Processing graph
    where PCM flows / in what order / where branches and taps exist
```

Never derive processing order from Fiber mount order or provider discovery order.

## Control plane builds; realtime executes

```text
CONTROL SIDE

Profile / desired graph
        |
        v
Composition Kernel
        |
instantiate / bind / withdraw plugins
        |
        v
ProcessingTopologyAuthority
        |
build validated immutable/pre-bound graph
        |
        v
RT-safe publication

----------------------------------------------

REALTIME / DATA SIDE

PcmBlock
  -> Decoder/SRC/Gain/EQ/... pre-bound entrypoints
  -> AudioOutput
```

Realtime execution must not perform:

```text
Context lookup
Capability resolution
Fiber lifecycle mutation
Reconcile
generic event dispatch
filesystem/network/UI
unbounded allocation/blocking
```

A data-plane callback may call a Plugin instance because the reference was already bound and published; that is not a Composition Kernel round trip.

## What becomes a Plugin

Default toward Plugin/Fiber when a stage is durable and independently meaningful:

```text
Decoder
SRC / Resampler
PlayerGain
Equalizer
Limiter / Compressor
Mixer
Analyzer
Recorder / Tap
AudioOutput
```

A stage earns its own plugin identity when it has enough of:

```text
independent configuration
independent replacement
independent failure/withdrawal domain
long-lived state/resource
stable capability contract
independent processing/data edge
```

Not every algorithmic helper is a Plugin. One Plugin may privately contain micro-stages that are not independently addressed by composition.

## What remains runtime state/data

```text
PcmBlock / MediaSpan / Buffer
GenerationId
Active / Prepared
Physical Fence transaction
TrackSession
DecodeSession
raw/derived evidence records
```

These do not get Profile/Fiber identity merely because they live for more than one function call.

## Playback authorities stay separate

```text
MusicKernel
    music/product meaning

TransportKernel
    playback time / windows / generation admission / fence semantics

ProcessingTopologyAuthority
    ordered PCM graph / graph publication

Composition Kernel
    plugin reachability / lifecycle / replacement

AudioOutput plugin
    device/session mechanism + physical evidence
```

`MusicKernel` / `TransportKernel` remain semantic-authority roles, not Composition plugins.

## TrackSession / DecodeSession with Decoder Plugin

```text
DecoderPlugin (durable mechanism provider)

TrackSession A
├── DecodeSession gen17 ---- provider-issued decoder handle A17
└── DecodeSession gen18 ---- provider-issued decoder handle A18
```

`TrackSession` still immediate-owns its DecodeSessions in the playback nested-resource tree. DecoderPlugin owns provider-global mechanism resources and must outlive any issued session handles that depend on it.

## PCM edge contract

Every direct edge must preserve enough typed truth for playback correctness:

```text
PCM format
frame count
MediaSpan / provenance
Generation provenance where required
bounded backpressure semantics
```

A buffer may be pooled/reused, but BufferId is never timeline authority.

## Parameter update vs graph change

```text
Gain 0.5 -> 0.7
    = plugin-local typed control update
    = not necessarily Reconcile

insert EQ / remove Analyzer / replace SRC / switch Output
    = durable composition change
    = Reconcile participants + rebuild/publish processing graph
```

Plugin identity is a lifecycle boundary, not a requirement to route every setting change through Composition Kernel.

## Withdrawal handshake for RT participants

```text
Plugin A withdrawing
        |
        v
exclude A from new resolution / new graph builds
        |
        v
build graph N+1 without A
        |
        v
publish graph N+1
        |
        v
stop admitting new audio quanta to graph N
        |
        v
wait old RT readers / queued references quiesce
        |
        v
release graph N refs to A
        |
        v
final-release Plugin A
```

Hard invariant:

> **No data-plane Plugin may be finally released while a published RT graph can still call it or retain a live reference to its data-plane endpoint.**

This is the bridge between generic K0 provider withdrawal and audio RT safety.

## Relationship to Physical Fence

Processing-graph retirement and playback Physical Fence are related but not identical:

```text
Graph quiescence
    protects plugin/resource lifetime

Physical Fence
    proves old submitted media can no longer continue audibly
```

A provider replacement may need both, but neither substitutes for the other.

TransportKernel remains the interpreter of fence evidence and generation admission.

## Failure isolation

A plugin loader/reconcile batch is not itself the failure domain.

```text
Analyzer optional + fails
    -> Analyzer unavailable
    -> healthy Decoder/Gain/Output remain if topology permits
```

Only declared mandatory dependencies/topology may propagate failure.

## Target real playback slice

After ADR-PBK-002 acceptance, the first real Windows slice should be:

```text
FFmpeg Decoder Plugin
        |
        | canonical PCM
        v
PlayerGain Plugin (or the smallest earned processing plugin set)
        |
        v
WASAPI AudioOutput Plugin
```

with Composition Kernel truly creating/binding/withdrawing those plugin instances and PCM travelling directly between their pre-bound endpoints.

The slice must still prove:

```text
Dual Window / Generation admission
Physical Fence
submitted != rendered
EOF != drain != ENDED
FactRevision freshness
TrackSession / DecodeSession ownership
RT firewall
provider withdrawal + graph quiescence
```

## Governance

Do not edit frozen `ARCH-004-plugin-graph-v1.mmd` or `ARCH-005-control-data-plane-v1.mmd` in place.

ADR-PBK-002 acceptance should earn new v2 diagrams and registry migration.
