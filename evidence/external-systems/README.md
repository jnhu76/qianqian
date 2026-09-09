# External systems

This directory stores failure evidence collected from systems outside Qianqian.

Read order for an explicit external-failure task:

1. `source-ledger.yml` — determine what has already been reviewed and what is actually new;
2. `failure-corpus.md` — read only when family classification or Qianqian disposition is needed;
3. `intake-protocol.md` — process for incremental discovery, delta review, dedup, and promotion.

For ordinary Qianqian ADR/architecture/formal/code review, do not load this directory.
