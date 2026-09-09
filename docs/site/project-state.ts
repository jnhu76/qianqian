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
  currentFrontier: 'Playback Foundations accepted (ADR-PBK-001); next: minimal PCM contract experiments under the accepted foundations',
  lastMilestone: 'Playback Foundations accepted after fresh-context adversarial review; legacy playback authority stays experimental evidence',

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
    'minimal PCM contract 实验最少需要哪些字段/语义边界（PcmBlock format/frames/time/provenance）？',
    '哪些 direct-flow / graph-publication 实验先挣得第一批 realtime 边界？',
    'tree-wide surfaces 是否仍把旧 playback 模型当 current authority（应为 experimental evidence only）？',
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
