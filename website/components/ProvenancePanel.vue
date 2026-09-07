<script setup lang="ts">
defineProps<{
  authority?: string[]
  decisions?: { issue?: number; pr?: number }[]
  implementation?: { issue?: number; pr?: number }[]
  evidence?: string[]
  lastVerified?: string
}>()

function githubIssue(n: number) {
  return `https://github.com/jnhu76/qianqian/issues/${n}`
}

function githubPr(n: number) {
  return `https://github.com/jnhu76/qianqian/pull/${n}`
}
</script>

<template>
  <div class="provenance-panel">
    <h4>Provenance</h4>

    <template v-if="authority?.length">
      <span class="provenance-label">Authority</span>
      <ul>
        <li v-for="a in authority" :key="a">
          <code>{{ a }}</code>
        </li>
      </ul>
    </template>

    <template v-if="decisions?.length">
      <span class="provenance-label">Decision History</span>
      <ul>
        <li v-for="d in decisions" :key="JSON.stringify(d)">
          <template v-if="d.issue">
            Issue <a :href="githubIssue(d.issue)" target="_blank">#{{ d.issue }}</a>
          </template>
          <template v-if="d.pr">
            PR <a :href="githubPr(d.pr)" target="_blank">#{{ d.pr }}</a>
          </template>
        </li>
      </ul>
    </template>

    <template v-if="implementation?.length">
      <span class="provenance-label">Implementation</span>
      <ul>
        <li v-for="imp in implementation" :key="JSON.stringify(imp)">
          <template v-if="imp.issue">
            Issue <a :href="githubIssue(imp.issue)" target="_blank">#{{ imp.issue }}</a>
          </template>
          <template v-if="imp.pr">
            PR <a :href="githubPr(imp.pr)" target="_blank">#{{ imp.pr }}</a>
          </template>
        </li>
      </ul>
    </template>

    <template v-if="evidence?.length">
      <span class="provenance-label">Evidence</span>
      <ul>
        <li v-for="e in evidence" :key="e">
          <code>{{ e }}</code>
        </li>
      </ul>
    </template>

    <template v-if="lastVerified">
      <span class="provenance-label">Last verified</span>
      <span> {{ lastVerified }}</span>
    </template>
  </div>
</template>
