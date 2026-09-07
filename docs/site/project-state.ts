/**
 * Single presentation-state authority for the Qianqian Engineering Observatory.
 *
 * PS1–PS7 rules:
 * - Homepage, roadmap and architecture-status UI consume this source.
 * - No duplicate "current frontier" strings across Markdown.
 * - Contains presentation status only; no architecture semantics.
 * - GitHub issue OPEN/CLOSED must not automatically mutate this.
 * - State changes are reviewed repository changes.
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
  currentFrontier: 'Playback Kernel',
  lastMilestone: 'Composition Kernel K0',

  layers: {
    playbackReference: 'HISTORICAL_EVIDENCE',
    ffmpegResearch: 'HISTORICAL_EVIDENCE',
    componentBoundary: 'FROZEN',
    baseKernel: 'IMPLEMENTED',
    playbackKernel: 'NEXT',
    decoder: 'PLANNED',
    processing: 'PLANNED',
    audioOutput: 'PLANNED',
    uiHost: 'DEFERRED',
  },

  nextQuestions: [
    'What is the minimum Playback Session contract?',
    'Where is Decoder / PCM ownership frozen?',
    'What capability must AudioOutput expose without leaking device policy?',
  ],
}

export const statusVocabulary: Record<Status, string> = {
  FROZEN: 'Frozen — architecture boundary accepted, no semantic changes allowed',
  IMPLEMENTED: 'Implemented — code merged and evidence verified',
  VALIDATED: 'Validated — experiment result confirmed by evidence',
  CURRENT: 'Current — active architecture boundary',
  NEXT: 'Next — immediate frontier',
  PLANNED: 'Planned — design not yet frozen',
  DEFERRED: 'Deferred — future consideration',
  HISTORICAL_EVIDENCE: 'Historical Evidence — preserved as opt-in evidence',
  SUPERSEDED: 'Superseded — replaced by a newer version',
}
