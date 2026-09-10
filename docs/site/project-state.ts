/**
 * Single presentation-state authority for the Qianqian Engineering Observatory.
 *
 * This file contains presentation state only; architecture semantics remain in
 * the relevant docs/ADR. GitHub issue state must not mutate these values
 * automatically. State changes are reviewed repository changes.
 */

export type Status =
  | 'FROZEN'
  | 'IMPLEMENTED'
  | 'VALIDATED'
  | 'CURRENT'
  | 'NEXT'
  | 'PLANNED'
  | 'DEFERRED'
  | 'HISTORICAL_EVIDENCE'
  | 'SUPERSEDED'

export interface ProjectState {
  currentFrontier: string
  lastMilestone: string
  layers: Record<string, Status>
  nextQuestions: string[]
}

export const projectState: ProjectState = {
  currentFrontier: 'Ladder evidence delivered through Phase C (PCM contract PR #93, direct flow PR #96) and realtime-view publication/reclamation mechanism evidence (PR #97, Issue #94 closed); next: re-audit the production realtime-runtime seam against the reconciled authority (PR #98), then mechanism adjudication (Phase D) and real decoder/output (Phase E)',
  lastMilestone: 'Realtime-view publication/reclamation mechanism evidence validated ADR §6 P1–P5 on a real Rust mechanism (PR #97); Realtime Runtime responsibility earned (Issue #94 closed)',

  layers: {
    playbackReference: 'HISTORICAL_EVIDENCE',
    ffmpegResearch: 'HISTORICAL_EVIDENCE',
    componentBoundary: 'HISTORICAL_EVIDENCE',
    baseKernel: 'IMPLEMENTED',
    playbackArchitecture: 'CURRENT',
    decoder: 'PLANNED',
    processing: 'PLANNED',
    audioOutput: 'PLANNED',
    uiHost: 'DEFERRED',
  },

  nextQuestions: [
    'production realtime-runtime seam（PR #98）如何按收口后的 authority/vocabulary（ADR-PBK-001 §16）重新审判：KEEP / SHRINK / REWORK / CLOSE？',
    '哪类 publication/reclamation mechanism 最终胜出（Phase D 机制裁决：RT acquire/release、queued-reference lifetime、final-drop、deferred disposal）？',
    '真实 decoder / output（Phase E）会挣得哪些 playback nouns？',
  ],
}

export const statusVocabulary: Record<Status, string> = {
  FROZEN: 'Frozen — architecture boundary accepted, no semantic changes allowed',
  IMPLEMENTED: 'Implemented — code merged and evidence verified',
  VALIDATED: 'Validated — experiment/result confirmed by evidence',
  CURRENT: 'Current — active design/reconciliation frontier',
  NEXT: 'Next — immediate future frontier',
  PLANNED: 'Planned — design/implementation not yet established',
  DEFERRED: 'Deferred — future consideration',
  HISTORICAL_EVIDENCE: 'Historical Evidence — preserved as opt-in evidence',
  SUPERSEDED: 'Superseded — replaced by a newer authority',
}
