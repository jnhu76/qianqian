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
  currentFrontier: 'Ladder evidence delivered through Phase C (PCM contract PR #93, direct flow PR #96) and realtime-view publication/reclamation mechanism evidence (PR #97, Issue #94 closed); production realtime seam re-audited and shrunk against the reconciled authority (PR #98, PRs #101–#103); next: mechanism adjudication (Phase D) and the first real decoder/output plugin work (Phase E)',
  lastMilestone: 'Production composition-root shrink completed: non-production playback baggage and the unearned pre-bound audio-output cache removed, runtime binding truth cleaned (PRs #101–#103)',

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
    '哪类 publication/reclamation mechanism 最终胜出（Phase D 机制裁决：RT acquire/release、queued-reference lifetime、final-drop、deferred disposal）？',
    '第一个真实 decoder/output 插件 vertical slice（Phase E）会挣得哪些合同与 playback nouns？',
    '第一个运行时图变更（换曲/换设备/seek 重开）何时触发 P1–P5 生产机制实现？',
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
