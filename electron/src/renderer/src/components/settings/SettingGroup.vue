<template>
  <div class="setting-group">
    <label class="group-title">{{ title }}</label>
    <div :class="[noCard ? '' : 'control-card']">
      <slot></slot>
    </div>
    <p v-if="tip" class="tip-text">{{ tip }}</p>
  </div>
</template>

<script setup lang="ts">
defineProps<{ title: string; tip?: string; noCard?: boolean }>()
</script>

<style scoped>
.setting-group {
  margin-bottom: 32px;
}
.group-title {
  display: block;
  font-size: 12px;
  font-weight: 700;
  color: var(--sys-text-tertiary);
  margin-bottom: 12px;
  margin-left: 4px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}
.control-card {
  background: var(--sys-surface);
  border-radius: 20px;
  padding: 8px 20px;
  border: 1px solid var(--sys-border);
}
.tip-text {
  font-size: 11px;
  color: var(--sys-text-tertiary);
  margin-top: 10px;
  padding-left: 4px;
}
</style>
