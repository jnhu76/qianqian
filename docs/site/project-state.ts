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
  currentFrontier: 'Playback Architecture — repository authority alignment / acceptance review',
  lastMilestone: 'Playback Architecture formal core PASS',

  layers: {
    playbackReference: 'HISTORICAL_EVIDENCE',
    ffmpegResearch: 'HISTORICAL_EVIDENCE',
    componentBoundary: 'FROZEN',
    baseKernel: 'IMPLEMENTED',
    playbackArchitecture: 'CURRENT',
    decoder: 'PLANNED',
    processing: 'PLANNED',
    audioOutput: 'PLANNED',
    uiHost: 'DEFERRED',
  },

  nextQuestions: [
    '当前 current surfaces 是否准确表达 ADR-PBK-001 为 PROPOSED replacement candidate，而 registry 的 ARCH-003 authority 尚未迁移？',
    'ADR-PBK-001 是否具备进入 ACCEPTED 的人工审查条件?',
    '只有在 ACCEPTED 之后：最小 executable playback model 应先挣得哪些 Rust representation?',
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
