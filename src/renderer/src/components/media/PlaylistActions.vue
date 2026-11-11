<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import AppIcon from '@renderer/components/common/AppIcon.vue'

defineProps<{ disabled?: boolean }>()
const emit = defineEmits<{ edit: []; delete: [] }>()
const root = ref<HTMLElement | null>(null)
const trigger = ref<HTMLButtonElement | null>(null)
const menu = ref<HTMLElement | null>(null)
const isOpen = ref(false)

const close = (restoreFocus = false): void => {
  isOpen.value = false
  if (restoreFocus) trigger.value?.focus()
}

const toggle = async (): Promise<void> => {
  isOpen.value = !isOpen.value
  if (isOpen.value) {
    await nextTick()
    menu.value?.querySelector<HTMLButtonElement>('button')?.focus()
  }
}

const select = (action: 'edit' | 'delete'): void => {
  close(true)
  if (action === 'edit') emit('edit')
  else emit('delete')
}

const onKeydown = (event: KeyboardEvent): void => {
  if (event.key === 'Escape') {
    event.preventDefault()
    close(true)
  } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    const items = Array.from(menu.value?.querySelectorAll<HTMLButtonElement>('button') ?? [])
    const current = items.indexOf(document.activeElement as HTMLButtonElement)
    items[(current + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length]?.focus()
  }
}

const onOutsideClick = (event: PointerEvent): void => {
  if (!root.value?.contains(event.target as Node)) close()
}
const onFocusOut = (event: FocusEvent): void => {
  if (!root.value?.contains(event.relatedTarget as Node)) close()
}

onMounted(() => document.addEventListener('pointerdown', onOutsideClick))
onBeforeUnmount(() => document.removeEventListener('pointerdown', onOutsideClick))
</script>

<template>
  <div ref="root" class="playlist-actions" @keydown="onKeydown" @focusout="onFocusOut">
    <button
      ref="trigger"
      class="menu-trigger"
      type="button"
      title="歌单操作"
      aria-label="歌单操作"
      aria-haspopup="menu"
      :aria-expanded="isOpen"
      :disabled="disabled"
      @click="toggle"
      @keydown.down.prevent.stop="!isOpen && toggle()"
    >
      <AppIcon name="more" :size="20" />
    </button>
    <Transition name="playlist-menu">
      <div v-if="isOpen" ref="menu" class="action-menu" role="menu" aria-label="歌单操作">
        <button type="button" role="menuitem" @click="select('edit')">
          <AppIcon name="edit" :size="17" />
          编辑歌单
        </button>
        <div class="menu-divider" role="separator"></div>
        <button class="delete-action" type="button" role="menuitem" @click="select('delete')">
          <AppIcon name="trash-outline" :size="17" />
          删除歌单
        </button>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.playlist-actions {
  position: relative;
  flex-shrink: 0;
}

.menu-trigger {
  display: grid;
  place-items: center;
  width: var(--sys-control-height);
  height: var(--sys-control-height);
  padding: 0;
  border: 1px solid var(--sys-border);
  border-radius: 50%;
  background: transparent;
  color: var(--sys-text-secondary);
  cursor: pointer;
  transition: background 0.15s;
}

.menu-trigger:hover,
.menu-trigger[aria-expanded='true'] {
  background: var(--sys-control-hover);
  color: var(--sys-text);
}

.menu-trigger:disabled {
  opacity: 0.45;
  cursor: default;
}

.action-menu {
  position: absolute;
  top: calc(100% + 8px);
  right: 0;
  z-index: 30;
  width: 176px;
  padding: 5px;
  border: 1px solid var(--glass-border);
  border-radius: 14px;
  background: var(--sys-bg-elevated);
  box-shadow: var(--sys-shadow-elevated);
  transform-origin: top right;
}

.action-menu button {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  height: 40px;
  padding: 0 12px;
  border: 0;
  border-radius: 9px;
  background: transparent;
  color: var(--sys-text);
  font: inherit;
  font-size: 13px;
  cursor: pointer;
  text-align: left;
}

.action-menu button:hover,
.action-menu button:focus-visible {
  background: var(--sys-control);
  outline: none;
}

.menu-divider {
  height: 1px;
  margin: 4px 8px;
  background: var(--sys-border);
}

.action-menu .delete-action {
  color: var(--sys-danger);
}

.action-menu .delete-action:hover,
.action-menu .delete-action:focus-visible {
  background: var(--sys-danger-soft);
}

.playlist-menu-enter-active,
.playlist-menu-leave-active {
  transition:
    opacity 0.12s,
    transform 0.12s;
}

.playlist-menu-enter-from,
.playlist-menu-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.97);
}
</style>
