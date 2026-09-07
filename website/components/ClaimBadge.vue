<script setup lang="ts">
defineProps<{
  role: 'authority' | 'evidence' | 'interpretation'
}>()
</script>

<template>
  <span :class="['claim-badge', `claim-badge--${role}`]">
    {{ role }}
  </span>
</template>
