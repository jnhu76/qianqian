<script lang="ts">
// Module-scope state shared by every MermaidDiagram instance on a page.
//
// Mermaid is a singleton module with site-global configuration: initialize()
// sets site-wide config, and render() "holds the diagram config scope for
// exactly as long as the render" (Mermaid public API). Two component
// instances rendering concurrently would therefore fight over that global
// scope. This module owns the single initialization authority and serializes
// every render through one queue, so components never touch global Mermaid
// state in parallel.
//
// Render ids must be unique across all diagrams in the document (not just
// per instance): Mermaid embeds the id in the SVG (marker/clipPath
// references), so per-instance counters would collide on multi-diagram pages.
type Mermaid = (typeof import('mermaid'))['default']

let globalRenderId = 0

let mermaidLoad: Promise<Mermaid> | null = null
let initializedTheme: 'dark' | 'default' | null = null
let renderChain: Promise<unknown> = Promise.resolve()

// securityLevel stays at Mermaid's default ('strict'): the site's diagrams
// only need <br/> line breaks in flowchart labels, which strict mode renders
// correctly (verified in browser); nothing on the site uses the click
// callbacks / unsanitized HTML that require 'loose'.
function siteConfig(theme: 'dark' | 'default') {
  return {
    startOnLoad: false,
    theme,
    flowchart: { useMaxWidth: true, htmlLabels: true, curve: 'basis' },
  }
}

// Load Mermaid once per document and initialize it for the given theme.
// Theme switches re-run initialize() with the full site config — the
// documented site-wide mechanism (there is no per-render config on render()).
// Only ever called from inside the serialized render queue below.
function acquireMermaid(theme: 'dark' | 'default'): Promise<Mermaid> {
  if (!mermaidLoad) {
    const load = import('mermaid').then((m) => {
      const mermaid = m.default
      mermaid.initialize(siteConfig(theme))
      initializedTheme = theme
      return mermaid
    })
    // A failed dynamic import (e.g. transient network error) must not poison
    // every later render: clear the cache so the next render retries.
    mermaidLoad = load
    load.catch(() => {
      if (mermaidLoad === load) mermaidLoad = null
    })
  }
  return mermaidLoad.then((mermaid) => {
    if (initializedTheme !== theme) {
      mermaid.initialize(siteConfig(theme))
      initializedTheme = theme
    }
    return mermaid
  })
}

// Serialize renders document-wide. Jobs run in FIFO order regardless of
// whether the previous job succeeded; failures propagate to the caller
// (localized error UI) without breaking the chain for other diagrams.
function enqueueRender<T>(job: () => Promise<T>): Promise<T> {
  const run = renderChain.then(job, job)
  renderChain = run.catch(() => {})
  return run
}
</script>

<script setup lang="ts">
/**
 * Qianqian-owned thin Mermaid integration seam (DOCS-BUILD-STABILITY-1).
 *
 * Markdown fenced ```mermaid blocks are intercepted in .vitepress/config.ts
 * and emitted as <MermaidDiagram code="...">. This component is the only
 * Qianqian code that touches the `mermaid` public package boundary; Mermaid's
 * internal dependency graph is opaque to this repository (the layering guard
 * lives in scripts/verify.mjs).
 *
 * Behavior parity with the retired third-party plugin path:
 * - client-only dynamic import; nothing Mermaid-related runs during SSR
 * - theme follows the site's `html.dark` class (default/light otherwise)
 * - re-renders when the theme toggles, on hard refresh and after client navigation
 * - a render failure is localized to the diagram, never blanks the app
 *
 * Lifecycle safety: rendering is asynchronous (dynamic import + render), so a
 * render may be superseded by unmount or by a newer render (e.g. theme
 * toggle) before it resolves. Every render carries an instance epoch and may
 * publish its result only if it is still the latest live render of this
 * mounted component.
 */
import { ref, onMounted, onBeforeUnmount } from 'vue'

const props = defineProps<{ code: string }>()

const container = ref<HTMLElement | null>(null)
const error = ref('')
let themeObserver: MutationObserver | null = null
let renderEpoch = 0
let disposed = false

function currentTheme(): 'dark' | 'default' {
  return document.documentElement.classList.contains('dark') ? 'dark' : 'default'
}

// True while `render` may still publish into this component.
function isLiveRender(epoch: number): boolean {
  return !disposed && epoch === renderEpoch && container.value !== null
}

async function renderDiagram() {
  const myEpoch = ++renderEpoch
  const theme = currentTheme()
  try {
    const { svg } = await enqueueRender(async () => {
      const mermaid = await acquireMermaid(theme)
      return mermaid.render(
        `qianqian-mermaid-${++globalRenderId}`,
        decodeURIComponent(props.code),
      )
    })
    if (!isLiveRender(myEpoch)) return
    container.value!.innerHTML = svg
  } catch (e) {
    if (!isLiveRender(myEpoch)) return
    container.value!.innerHTML = ''
    error.value = e instanceof Error ? e.message : String(e)
  }
}

onMounted(() => {
  themeObserver = new MutationObserver(() => {
    void renderDiagram()
  })
  themeObserver.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['class'],
  })
  void renderDiagram()
})

onBeforeUnmount(() => {
  // Invalidate any in-flight render so it cannot publish after unmount.
  disposed = true
  renderEpoch++
  themeObserver?.disconnect()
  themeObserver = null
})
</script>

<template>
  <div class="mermaid-diagram">
    <div v-if="error" class="mermaid-error">
      <p><strong>Mermaid render error</strong></p>
      <pre>{{ error }}</pre>
    </div>
    <div ref="container" />
  </div>
</template>

<style>
.mermaid-diagram {
  margin: 16px 0;
  text-align: center;
  overflow-x: auto;
}

.mermaid-diagram svg {
  max-width: 100%;
  height: auto;
}

.mermaid-error {
  border: 1px solid #f0a4a4;
  border-radius: 6px;
  background: rgba(255, 0, 0, 0.06);
  color: #c0392b;
  padding: 8px 12px;
  text-align: left;
}

.mermaid-error p {
  margin: 0 0 4px;
}

.mermaid-error pre {
  margin: 0;
  white-space: pre-wrap;
  font-size: 0.85em;
}
</style>
