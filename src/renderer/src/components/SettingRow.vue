<template>
  <div class="control-row">
    <div class="info">
      <span class="primary">{{ title }}</span>
      <span v-if="description" class="secondary">{{ description }}</span>
    </div>
    <div class="action">
      <slot></slot>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{ title: string, description?: string }>();
</script>

<style scoped>
.control-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 0;
}
.control-row:not(:last-child) {
  border-bottom: 1px solid rgba(0, 0, 0, 0.04);
}
.info { display: flex; flex-direction: column; gap: 2px; }
.primary { font-size: 15px; font-weight: 600; color: #111; }
.secondary { font-size: 12px; color: rgba(0, 0, 0, 0.4); }
</style>
