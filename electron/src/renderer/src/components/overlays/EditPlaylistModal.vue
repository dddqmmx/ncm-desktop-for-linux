<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import AppIcon from '@renderer/components/common/AppIcon.vue'
import SongCover from '@renderer/components/media/SongCover.vue'
import {
  MAX_PLAYLIST_DESCRIPTION_LENGTH,
  usePlaylistStore,
  type EditablePlaylist,
  type PlaylistMetadata
} from '@renderer/stores/playlistStore'
import { MAX_PLAYLIST_NAME_LENGTH, normalizePlaylistName } from '@renderer/utils/playlistName'

const props = defineProps<{
  playlist: EditablePlaylist
  coverUrl?: string
  trackCount?: number
}>()
const emit = defineEmits<{
  (e: 'close'): void
  (e: 'saved', playlistId: number, metadata: PlaylistMetadata): void
}>()

const playlistStore = usePlaylistStore()
const dialog = ref<HTMLDialogElement | null>(null)
const nameInput = ref<HTMLInputElement | null>(null)
const name = ref(props.playlist.name)
const description = ref(props.playlist.description ?? '')
const submitting = ref(false)
const errorMessage = ref('')
const hasChanges = computed(
  () =>
    normalizePlaylistName(name.value) !== props.playlist.name ||
    description.value.trim() !== (props.playlist.description ?? '')
)
const canSubmit = computed(
  () =>
    !submitting.value &&
    hasChanges.value &&
    !!normalizePlaylistName(name.value) &&
    description.value.length <= MAX_PLAYLIST_DESCRIPTION_LENGTH &&
    playlistStore.canManagePlaylist(props.playlist)
)

onMounted(() => {
  dialog.value?.showModal()
  nameInput.value?.focus()
  nameInput.value?.select()
})

const close = (): void => {
  if (!submitting.value) emit('close')
}

const trapFocus = (event: KeyboardEvent): void => {
  const controls = Array.from(
    dialog.value?.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled), textarea:not(:disabled)'
    ) ?? []
  )
  const first = controls[0]
  const last = controls.at(-1)
  if (!first || !last) {
    event.preventDefault()
    return
  }
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault()
    last.focus()
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault()
    first.focus()
  }
}

const submit = async (): Promise<void> => {
  if (!canSubmit.value) return
  submitting.value = true
  errorMessage.value = ''
  const playlist = props.playlist
  try {
    const metadata = await playlistStore.updatePlaylist(playlist, {
      name: name.value,
      description: description.value
    })
    emit('saved', playlist.id, metadata)
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : '保存歌单失败，请重试'
  } finally {
    submitting.value = false
  }
}
</script>

<template>
  <Teleport to="body">
    <dialog
      ref="dialog"
      class="edit-playlist-dialog"
      aria-labelledby="edit-playlist-title"
      :aria-busy="submitting"
      @cancel.prevent="close"
      @click.self="close"
      @keydown.tab="trapFocus"
    >
      <section class="edit-playlist-card">
        <header class="card-header">
          <h2 id="edit-playlist-title">编辑歌单</h2>
          <button
            class="close-btn"
            type="button"
            aria-label="关闭"
            :disabled="submitting"
            @click="close"
          >
            <AppIcon name="close" :size="18" />
          </button>
        </header>

        <div class="playlist-preview">
          <div class="preview-cover"><SongCover :id="coverUrl" size="160y160" /></div>
          <div class="preview-info">
            <span class="preview-name" :title="playlist.name">{{ playlist.name }}</span>
            <span class="preview-meta"
              >歌单<span v-if="trackCount !== undefined"> · {{ trackCount }} 首歌曲</span></span
            >
          </div>
        </div>

        <form @submit.prevent="submit">
          <div class="form-fields">
            <label class="field">
              <span class="field-heading"
                ><span>名称</span
                ><span class="character-count"
                  >{{ name.length }} / {{ MAX_PLAYLIST_NAME_LENGTH }}</span
                ></span
              >
              <input
                ref="nameInput"
                v-model="name"
                type="text"
                :maxlength="MAX_PLAYLIST_NAME_LENGTH"
                :disabled="submitting"
                placeholder="给歌单起个名字"
                required
              />
            </label>
            <label class="field">
              <span class="field-heading"
                ><span>简介 <span class="optional-label">选填</span></span
                ><span class="character-count"
                  >{{ description.length }} / {{ MAX_PLAYLIST_DESCRIPTION_LENGTH }}</span
                ></span
              >
              <textarea
                v-model="description"
                :maxlength="MAX_PLAYLIST_DESCRIPTION_LENGTH"
                :disabled="submitting"
                placeholder="记录这份歌单的心情、故事或灵感…"
                rows="4"
              ></textarea>
            </label>
            <p v-if="errorMessage" class="error-message" role="alert">{{ errorMessage }}</p>
          </div>
          <footer class="card-actions">
            <button class="btn btn-secondary" type="button" :disabled="submitting" @click="close">
              取消
            </button>
            <button class="btn btn-primary" type="submit" :disabled="!canSubmit">
              <span v-if="submitting" class="saving-spinner" aria-hidden="true"></span>
              {{ submitting ? '保存中…' : '保存修改' }}
            </button>
          </footer>
        </form>
      </section>
    </dialog>
  </Teleport>
</template>

<style scoped>
.edit-playlist-dialog {
  width: min(480px, calc(100vw - 40px));
  max-height: calc(100dvh - 48px);
  margin: auto;
  padding: 0;
  overflow-x: hidden;
  border: 1px solid var(--glass-border);
  border-radius: 22px;
  background: var(--sys-bg-elevated);
  color: var(--sys-text);
  box-shadow:
    var(--sys-shadow-elevated),
    0 0 0 1px var(--sys-border);
  -webkit-app-region: no-drag;
}

.edit-playlist-dialog::backdrop {
  background: rgba(12, 15, 24, 0.32);
  backdrop-filter: blur(6px);
}

.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 24px 28px 20px;
}

h2 {
  font-size: 19px;
  font-weight: 700;
  letter-spacing: -0.4px;
}

.close-btn {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 0;
  border-radius: 50%;
  background: var(--sys-control);
  color: var(--sys-text-tertiary);
  cursor: pointer;
}

.close-btn:hover:not(:disabled) {
  background: var(--sys-control-hover);
  color: var(--sys-text);
}

.playlist-preview {
  display: flex;
  align-items: center;
  gap: 14px;
  margin: 0 28px 24px;
}

.preview-cover {
  flex-shrink: 0;
  width: 64px;
  height: 64px;
  border-radius: 10px;
  background: var(--sys-control);
  box-shadow: 0 3px 10px rgba(0, 0, 0, 0.08);
}

.preview-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.preview-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 15px;
  font-weight: 600;
}

.preview-meta {
  font-size: 12px;
  color: var(--sys-text-tertiary);
}

.form-fields {
  display: flex;
  flex-direction: column;
  gap: 20px;
  padding: 0 28px 28px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.field-heading {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  font-size: 13px;
  color: var(--sys-text-secondary);
}

.optional-label,
.character-count {
  font-size: 11px;
  color: var(--sys-text-tertiary);
  font-variant-numeric: tabular-nums;
}

.optional-label {
  margin-left: 4px;
}

input,
textarea {
  width: 100%;
  padding: 11px 13px;
  border: 1px solid var(--sys-border);
  border-radius: 10px;
  background: var(--sys-control);
  color: var(--sys-text);
  font: inherit;
  font-size: 13px;
  line-height: 1.65;
  outline: none;
  transition:
    border-color 0.15s,
    box-shadow 0.15s;
}

input {
  height: 44px;
}

textarea {
  min-height: 112px;
  max-height: 220px;
  resize: vertical;
}

input::placeholder,
textarea::placeholder {
  color: var(--sys-text-tertiary);
}

input:focus,
textarea:focus {
  border-color: var(--theme-color);
  box-shadow: 0 0 0 3px var(--theme-color-soft);
}

.error-message {
  padding: 10px 12px;
  border-radius: 10px;
  background: var(--sys-danger-soft);
  color: var(--sys-danger);
  font-size: 12px;
}

.card-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 16px 28px;
  border-top: 1px solid var(--sys-border);
  background: var(--sys-bg-soft);
}

.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  min-width: 76px;
  height: 36px;
  padding: 0 18px;
  border: 1px solid transparent;
  border-radius: 9px;
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition:
    background 0.15s,
    opacity 0.15s;
}

.btn-secondary {
  background: transparent;
  color: var(--sys-text-secondary);
}

.btn-secondary:hover:not(:disabled) {
  background: var(--sys-control-hover);
}

.btn-primary {
  background: var(--theme-color);
  color: var(--sys-on-accent);
}

.btn-primary:hover:not(:disabled) {
  background: var(--theme-color-strong);
}

button:disabled {
  opacity: 0.45;
  cursor: default;
}

.saving-spinner {
  width: 12px;
  height: 12px;
  border: 1.5px solid currentColor;
  border-right-color: transparent;
  border-radius: 50%;
  animation: spin 0.7s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    transition: none;
    animation: none;
  }
}
</style>
