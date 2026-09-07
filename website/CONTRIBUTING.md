# Observatory Contribution Guide

The Qianqian Engineering Observatory is a curated projection of repository truth. These rules keep it that way.

---

## Rules

### 1. Do not create architecture truth in Web-only prose.

The Observatory explains accepted architecture. It does not create it.

### 2. Every architecture page requires authority.

Every architecture page must carry a `<ProvenancePanel>` linking to canonical authority docs.

### 3. Every experiment claim requires evidence.

Quantitative claims must point to test results, artifacts, or executable evidence.

### 4. Formal diagrams use registered Mermaid assets.

Canonical architecture diagrams are stored in `docs/architecture/diagrams/` and registered in `docs/architecture/registry.yml`.

Current reality: several pages still embed inline Mermaid graphs that are simplified, canonical-equivalent copies of registered diagrams (e.g. ARCH-001/002/004/005 cores). The intended contract — pages reference registry IDs and carry no duplicated canonical source (AR7) — is documented but **not yet enforced** by `docs:verify`. Inline Mermaid remains acceptable only for explanatory diagrams that are not registered architecture assets.

### 5. Frozen diagrams are versioned, not edited semantically.

If architecture changes, create a new version (`ARCH-002-v2.mmd`). Do not mutate a frozen v1.

### 6. GitHub state is not semantic status.

Issue OPEN/CLOSED is workflow state. Observatory statuses are: FROZEN, IMPLEMENTED, VALIDATED, CURRENT, NEXT, PLANNED, DEFERRED, HISTORICAL_EVIDENCE, SUPERSEDED.

### 7. Project status comes from project-state.ts.

Homepage, roadmap, and architecture-status UI consume the same source. No duplicate status strings.

### 8. Machine facts prefer machine artifacts.

If artifact files disagree with issue text, machine artifacts win. Issue text provides interpretation/provenance.

### 9. Web prose is a projection, not authority.

The canonical architecture lives in `docs/architecture/`. The Observatory curates and explains it.

### 10. New major architecture must pass its own design gate before Observatory status changes.

Don't change a page's status badge because you implemented something. The architecture gate must pass first.

---

## Adding a New Architecture Page

1. Ensure the architecture has passed its design gate
2. Register the diagram in `docs/architecture/registry.yml`
3. Create the diagram in `docs/architecture/diagrams/`
4. Create the page with `<ProvenancePanel>` linking to authority
5. Add to sidebar in `.vitepress/config.ts`
6. Run `pnpm docs:verify` to check invariants

---

## Adding a New Experiment Page

1. Register in `docs/experiments/registry.yml`
2. Follow the standard contract: Question → Baseline → Hypothesis → Method → Evidence → Result → Architectural Consequence
3. Link evidence to tests/artifacts
4. Use <ClaimBadge role="evidence" /> for measured facts

---

## Status Vocabulary

Use only these status values:

| Status | Meaning |
|--------|---------|
| FROZEN | Architecture boundary accepted |
| IMPLEMENTED | Code merged and evidence verified |
| VALIDATED | Experiment result confirmed |
| CURRENT | Active architecture boundary |
| NEXT | Immediate frontier |
| PLANNED | Design not yet frozen |
| DEFERRED | Future consideration |
| HISTORICAL_EVIDENCE | Preserved as opt-in evidence |
| SUPERSEDED | Replaced by newer version |

Do not invent synonyms.
