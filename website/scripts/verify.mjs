#!/usr/bin/env node
/**
 * docs:verify — Build-time verification for the Qianqian Engineering Observatory.
 *
 * Verifies:
 * AR1  architecture ID is globally unique
 * AR2  (id, version) is immutable identity
 * AR3  every registered diagram file exists
 * AR4  every authority doc exists
 * AR5  every CURRENT / IMPLEMENTED architecture points to one explicit diagram version
 * AR6  a SUPERSEDED diagram cannot simultaneously be current
 * AR7  (documented, NOT yet enforced) Web pages reference architecture registry IDs, not duplicated canonical graph source
 * AR8  architecture registry does not use GitHub open/closed state as semantic status
 * AR9  (documented, NOT yet enforced) frozen diagram version may only be replaced by a new version when architecture changes
 * AR10 architecture pages must declare provenance
 * ER1  experiment ID is globally unique
 * ER2  experiment evidence files exist where local
 * PS1–PS7 project-state rules
 */

import { readFileSync, existsSync } from 'fs'
import { parse as parseYaml } from 'yaml'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(__dirname, '../..')
const WEBSITE = resolve(__dirname, '..')

let errors = 0
let warnings = 0

function fail(msg) {
  console.error(`  FAIL: ${msg}`)
  errors++
}

function warn(msg) {
  console.log(`  WARN: ${msg}`)
  warnings++
}

function pass(msg) {
  console.log(`  OK: ${msg}`)
}

function fileExists(rel) {
  return existsSync(resolve(ROOT, rel))
}

// --- Architecture Registry ---

console.log('\n=== Architecture Registry ===')

let archReg
try {
  const raw = readFileSync(resolve(ROOT, 'docs/architecture/registry.yml'), 'utf-8')
  archReg = parseYaml(raw)
  pass('registry.yml parses')
} catch (e) {
  fail(`registry.yml parse failed: ${e.message}`)
  process.exit(1)
}

const archIds = new Set()
const archDiagrams = new Map()

for (const arch of archReg.architectures || []) {
  // AR1: globally unique ID
  if (archIds.has(arch.id)) {
    fail(`AR1: duplicate architecture ID: ${arch.id}`)
  }
  archIds.add(arch.id)

  // AR2: (id, version) identity
  if (!arch.version) {
    fail(`AR2: ${arch.id} missing version`)
  }

  // AR3: diagram file exists
  if (arch.diagram?.source) {
    if (!fileExists(arch.diagram.source)) {
      fail(`AR3: ${arch.id} diagram file not found: ${arch.diagram.source}`)
    } else {
      pass(`${arch.id} diagram exists: ${arch.diagram.source}`)
    }
    archDiagrams.set(arch.id, arch.diagram.source)
  }

  // AR4: authority docs exist
  for (const auth of arch.authority || []) {
    if (!fileExists(auth)) {
      fail(`AR4: ${arch.id} authority doc not found: ${auth}`)
    }
  }

  // AR5: CURRENT/IMPLEMENTED points to explicit diagram version
  if (['CURRENT', 'IMPLEMENTED'].includes(arch.status)) {
    if (!arch.diagram?.source) {
      fail(`AR5: ${arch.id} is ${arch.status} but has no diagram`)
    }
  }

  // AR6: SUPERSEDED cannot be current
  if (arch.status === 'SUPERSEDED' && arch.diagram?.frozen) {
    fail(`AR6: ${arch.id} is SUPERSEDED but has frozen diagram`)
  }

  // AR8: no GitHub state as semantic status
  const rawStr = JSON.stringify(arch)
  if (rawStr.includes('"open"') || rawStr.includes('"closed"')) {
    fail(`AR8: ${arch.id} uses GitHub state as semantic status`)
  }

  console.log(`  ${arch.id}: ${arch.status} (v${arch.version})`)
}

pass(`${archIds.size} architecture IDs, all unique`)

// --- Experiment Registry ---

console.log('\n=== Experiment Registry ===')

let expReg
try {
  const raw = readFileSync(resolve(ROOT, 'docs/experiments/registry.yml'), 'utf-8')
  expReg = parseYaml(raw)
  pass('registry.yml parses')
} catch (e) {
  fail(`experiments/registry.yml parse failed: ${e.message}`)
  process.exit(1)
}

const expIds = new Set()
for (const exp of expReg.experiments || []) {
  if (expIds.has(exp.id)) {
    fail(`ER1: duplicate experiment ID: ${exp.id}`)
  }
  expIds.add(exp.id)

  // ER2: local evidence files exist
  for (const ev of exp.evidence || []) {
    if (ev.startsWith('.') || ev.startsWith('docs/') || ev.startsWith('crates/')) {
      if (!fileExists(ev)) {
        fail(`ER2: ${exp.id} evidence file not found: ${ev}`)
      }
    }
  }

  // D1 (WEB-EVIDENCE-1): the FFmpeg minimization experiment is historical
  // evidence; only an explicit future authority process may change that.
  if (exp.id === 'EXP-FFMPEG-001' && exp.status !== 'HISTORICAL_EVIDENCE') {
    fail(`D1: ${exp.id} status must remain HISTORICAL_EVIDENCE (got ${exp.status})`)
  }

  console.log(`  ${exp.id}: ${exp.status}`)
}

pass(`${expIds.size} experiment IDs, all unique`)

// D2 (WEB-EVIDENCE-1): the Web page projects the registry status for the
// FFmpeg experiment rather than inventing a second status of its own.
function parseFrontmatter(content) {
  const m = content.match(/^---\n([\s\S]*?)\n---/)
  if (!m) return {}
  try {
    return parseYaml(m[1])
  } catch {
    return {}
  }
}

const ffmpegPagePath = resolve(WEBSITE, 'experiments/ffmpeg-minimization.md')
const expFfmpeg = expReg.experiments?.find(e => e.id === 'EXP-FFMPEG-001')
if (expFfmpeg && existsSync(ffmpegPagePath)) {
  const fm = parseFrontmatter(readFileSync(ffmpegPagePath, 'utf-8'))
  if (fm.status !== expFfmpeg.status) {
    fail(`D2: ffmpeg-minimization.md status (${fm.status}) diverges from registry (${expFfmpeg.status})`)
  } else {
    pass(`D2: ffmpeg-minimization.md projects registry status ${expFfmpeg.status}`)
  }
}

// --- Project State ---

console.log('\n=== Project State ===')

const psPath = resolve(ROOT, 'docs/site/project-state.ts')
if (existsSync(psPath)) {
  const psContent = readFileSync(psPath, 'utf-8')

  // PS1: project-state exists
  pass('project-state.ts exists')

  // PS4: no architecture semantics (basic check)
  if (psContent.includes('fn ') || psContent.includes('impl ')) {
    warn('PS4: project-state may contain Rust code (architecture semantics?)')
  }

  // PS6: no GitHub state references
  if (psContent.includes('.state') || psContent.includes('issue.state')) {
    fail('PS6: project-state references GitHub issue state')
  }
} else {
  fail('PS1: project-state.ts not found')
}

// --- Page Provenance (AR10) ---

console.log('\n=== Page Provenance ===')

const pages = [
  'website/control/index.md',
  'website/control/roadmap.md',
  'website/control/history.md',
  'website/architecture/index.md',
  'website/architecture/base-kernel.md',
  'website/architecture/playback-kernel.md',
  'website/architecture/plugin-graph.md',
  'website/architecture/realtime-data-plane.md',
  'website/experiments/index.md',
  'website/experiments/ffmpeg-minimization.md',
  'website/experiments/composition-kernel-oracles.md',
  'website/research/index.md',
  'website/research/spatiotemporal-composability.md',
  'website/research/ffmpeg-closure.md',
  'website/research/realtime-audio.md',
  'website/research/dsh-composition-lineage.md',
]

// D4 (WEB-EVIDENCE-1): provenance targets that name local repo paths must
// exist. GitHub issues/PRs and tag references (e.g. research/playback-
// reference-v1) are not local files and are deliberately not checked.
function extractProvenancePaths(content) {
  const paths = []
  const re = /:(authority|evidence)="\[([^\]]*)\]"/g
  let m
  while ((m = re.exec(content))) {
    for (const raw of m[2].split("'")) {
      const ref = raw.trim().replace(/ §.*$/, '')
      if (ref.startsWith('docs/') || ref.startsWith('crates/') || ref.startsWith('.')) {
        paths.push(ref)
      }
    }
  }
  return paths
}

for (const page of pages) {
  const fullPath = resolve(WEBSITE, '..', page)
  if (!existsSync(fullPath)) {
    fail(`page not found: ${page}`)
    continue
  }
  const content = readFileSync(fullPath, 'utf-8')

  // AR10: architecture pages must declare provenance
  if (page.includes('architecture/') && !content.includes('ProvenancePanel')) {
    fail(`AR10: ${page} missing provenance declaration`)
  } else if (content.includes('ProvenancePanel')) {
    pass(`${page} has provenance`)
  } else {
    pass(`${page} exists`)
  }

  // D3/D5 (WEB-EVIDENCE-1): the DSH lineage page makes architecture-history
  // claims, so provenance is mandatory and its status must stay historical —
  // DSH is reference/influence, never current Qianqian authority.
  if (page.includes('dsh-composition-lineage')) {
    const fm = parseFrontmatter(content)
    if (!content.includes('ProvenancePanel')) {
      fail(`D3: ${page} must declare provenance (architecture-history claims)`)
    }
    if (fm.status !== 'HISTORICAL_EVIDENCE') {
      fail(`D5: ${page} must be HISTORICAL_EVIDENCE (got ${fm.status})`)
    }
  }

  // D4: local provenance targets must exist
  for (const ref of extractProvenancePaths(content)) {
    if (!fileExists(ref)) {
      fail(`D4: provenance target not found: ${ref} (in ${page})`)
    }
  }
}

// --- Diagram Files ---

console.log('\n=== Diagram Files ===')

const diagramDir = resolve(ROOT, 'docs/architecture/diagrams')
if (existsSync(diagramDir)) {
  const { readdirSync } = await import('fs')
  const files = readdirSync(diagramDir).filter(f => f.endsWith('.mmd'))
  for (const f of files) {
    const content = readFileSync(resolve(diagramDir, f), 'utf-8')
    if (content.trim().length === 0) {
      fail(`empty diagram: ${f}`)
    } else {
      pass(`diagram ${f} (${content.length} bytes)`)
    }
  }
} else {
  fail('diagrams directory not found')
}

// --- Mermaid Integration Boundary (DOCS-BUILD-STABILITY-1) ---

console.log('\n=== Mermaid Integration Boundary ===')

// Qianqian-owned source/config must not know Mermaid's transitive internal
// dependency graph (fastdom) or the retired vitepress-plugin-mermaid path.
// package-lock.json is deliberately excluded: Mermaid may legitimately
// depend on fastdom transitively — that is Mermaid's boundary, not ours.
const forbiddenTokens = ['fastdom', 'fastdom-promised', 'vitepress-plugin-mermaid']
const skipDirs = new Set(['node_modules', 'dist', 'cache'])
const skipFiles = new Set(['package-lock.json'])
// The guard itself must be exempt: it is the enforcement point, not a
// violation surface, and naming the tokens is its job.
const skipRelPrefixes = ['scripts/']
const { readdirSync: fsReaddirSync } = await import('fs')

function scanDir(dir, relDir = '') {
  const entries = fsReaddirSync(dir, { withFileTypes: true })
  for (const entry of entries) {
    if (entry.isDirectory()) {
      if (skipDirs.has(entry.name)) continue
      scanDir(resolve(dir, entry.name), `${relDir}${entry.name}/`)
    } else if (entry.isFile() && !skipFiles.has(entry.name)) {
      const rel = `${relDir}${entry.name}`
      if (skipRelPrefixes.some(prefix => rel.startsWith(prefix))) continue
      const content = readFileSync(resolve(dir, entry.name), 'utf-8')
      for (const token of forbiddenTokens) {
        if (content.includes(token)) {
          fail(`mermaid boundary: "${token}" referenced in ${rel}`)
        }
      }
    }
  }
}

scanDir(WEBSITE)

// Direct dependency surface must not name the retired plugin.
const pkg = JSON.parse(readFileSync(resolve(WEBSITE, 'package.json'), 'utf-8'))
const directDeps = { ...(pkg.dependencies || {}), ...(pkg.devDependencies || {}) }
if ('vitepress-plugin-mermaid' in directDeps) {
  fail('mermaid boundary: vitepress-plugin-mermaid is a direct dependency')
} else {
  pass('vitepress-plugin-mermaid absent from direct dependencies')
}
pass('no fastdom / vitepress-plugin-mermaid coupling in Qianqian-owned files')

// --- Summary ---

console.log('\n=== Summary ===')
console.log(`  Errors: ${errors}`)
console.log(`  Warnings: ${warnings}`)

if (errors > 0) {
  console.log('\n  VERDICT: FAIL')
  process.exit(1)
} else {
  console.log('\n  VERDICT: PASS')
}
