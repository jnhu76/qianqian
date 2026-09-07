<script setup lang="ts">
defineProps<{
  status: string
}>()
</script>

<template>
  <span :class="['status-badge', `status-badge--${status.toLowerCase()}`]">
    {{ status }}
  </span>
</template>
