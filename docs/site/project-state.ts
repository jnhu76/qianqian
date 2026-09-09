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
  currentFrontier: 'Playback Architecture — deterministic executable temporal core (implementation entry)',
  lastMilestone: 'ADR-PBK-001 ACCEPTED; ARCH-003 authority migrated',

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
    '当前 current surfaces 是否准确表达 ADR-PBK-001 为 ACCEPTED 且为已登记的 ARCH-003 authority（同时不把架构接受误写成实现完成）？',
    'deterministic executable temporal core 应先挣得哪些 Rust representation（不预先冻结 struct/module layout）？',
    '施工发现协议：哪些实现压力属于 Class A/B，哪些必须走 Class C 的 ADR+specs+tests 同步事务？',
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
