<script lang="ts">
// Module-scope state shared by every MermaidDiagram instance on a page.
// Render ids must be unique across all diagrams in the document: Mermaid
// embeds the id in the SVG (marker/clipPath references), so per-instance
// counters would collide on multi-diagram pages.
let globalRenderId = 0
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
 */
import { ref, onMounted, onBeforeUnmount } from 'vue'

const props = defineProps<{ code: string }>()

const container = ref<HTMLElement | null>(null)
const error = ref('')
let themeObserver: MutationObserver | null = null

function currentTheme(): 'dark' | 'default' {
  return document.documentElement.classList.contains('dark') ? 'dark' : 'default'
}

async function renderDiagram() {
  if (!container.value) return
  error.value = ''
  try {
    const mermaid = (await import('mermaid')).default
    mermaid.initialize({
      startOnLoad: false,
      theme: currentTheme(),
      securityLevel: 'loose',
      flowchart: { useMaxWidth: true, htmlLabels: true, curve: 'basis' },
    })
    const { svg } = await mermaid.render(
      `qianqian-mermaid-${++globalRenderId}`,
      decodeURIComponent(props.code),
    )
    container.value.innerHTML = svg
  } catch (e) {
    container.value.innerHTML = ''
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
