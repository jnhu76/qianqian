<script setup lang="ts">
defineProps<{
  role: 'authority' | 'evidence' | 'interpretation'
}>()

// Display labels are localized; the stored `role` prop value stays the
// machine-readable vocabulary (authority / evidence / interpretation).
const labels = {
  authority: '权威',
  evidence: '证据',
  interpretation: '解读',
} as const
</script>

<template>
  <span :class="['claim-badge', `claim-badge--${role}`]" :title="role">
    {{ labels[role] }}
  </span>
</template>
