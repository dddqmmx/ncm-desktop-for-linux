<script setup lang="ts">
import { storeToRefs } from 'pinia'
import { useDialogStore } from '@renderer/stores/dialogStore'

const dialogStore = useDialogStore()
const { isOpen, title, message, mode, confirmText, cancelText } = storeToRefs(dialogStore)
</script>

<template>
  <Transition name="dialog-fade">
    <div v-if="isOpen" class="dialog-layer" @click.self="dialogStore.cancel">
      <section class="app-dialog liquid-glass" role="dialog" aria-modal="true" :aria-label="title">
        <div class="dialog-content">
          <h2>{{ title }}</h2>
          <p>{{ message }}</p>
        </div>
        <div class="dialog-actions">
          <button
            v-if="mode === 'confirm-cancel'"
            class="dialog-btn dialog-btn-secondary"
            type="button"
            @click="dialogStore.cancel"
          >
            {{ cancelText }}
          </button>
          <button class="dialog-btn dialog-btn-primary" type="button" @click="dialogStore.confirm">
            {{ confirmText }}
          </button>
        </div>
      </section>
    </div>
  </Transition>
</template>

<style scoped>
.dialog-layer {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: grid;
  place-items: center;
  padding: 28px;
  background: rgba(0, 0, 0, 0.18);
}

.app-dialog {
  width: min(520px, calc(100vw - 40px));
  min-height: 188px;
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 16px;
  padding: 22px;
  border: 1px solid var(--sys-border);
  border-radius: 20px;
  background: var(--sys-surface-strong);
  color: var(--sys-text);
  box-shadow: var(--sys-shadow-elevated);
  backdrop-filter: var(--sys-glass-blur);
  -webkit-backdrop-filter: var(--sys-glass-blur);
}

.dialog-content {
  flex: 1;
  min-width: 0;
}

.dialog-content h2 {
  margin: 0 0 8px;
  color: var(--sys-text);
  font-size: 18px;
  font-weight: 700;
  line-height: 1.35;
}

.dialog-content p {
  margin: 0;
  color: var(--sys-text-secondary);
  font-size: 14px;
  line-height: 1.65;
  white-space: pre-wrap;
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.dialog-btn {
  min-width: 76px;
  height: 40px;
  padding: 0 18px;
  border: 0;
  border-radius: 999px;
  font-size: 14px;
  font-weight: 700;
  cursor: pointer;
}

.dialog-btn-primary {
  color: var(--sys-on-accent);
  background: var(--theme-color);
}

.dialog-btn-secondary {
  color: var(--sys-text);
  background: var(--sys-control);
}

.dialog-btn:hover {
  filter: brightness(0.98);
}

.dialog-fade-enter-active,
.dialog-fade-leave-active {
  transition: opacity 0.18s ease;
}

.dialog-fade-enter-from,
.dialog-fade-leave-to {
  opacity: 0;
}

@media (max-width: 560px) {
  .app-dialog {
    border-radius: 28px;
    padding: 20px;
  }
}
</style>
