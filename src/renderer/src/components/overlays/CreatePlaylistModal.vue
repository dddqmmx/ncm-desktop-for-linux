<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import { computed, ref, nextTick } from 'vue'
import { useUserStore } from '@renderer/stores/userStore'
import { apiErrorMessage, isApiSuccess, type ServiceResult } from '@renderer/utils/api'
import { MAX_PLAYLIST_NAME_LENGTH, normalizePlaylistName } from '@renderer/utils/playlistName'

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'created', playlistId: number): void
}>()

const userStore = useUserStore()

const name = ref('')
const isPrivate = ref(false)
const submitting = ref(false)
const errorMessage = ref('')
const nameInput = ref<HTMLInputElement | null>(null)

const normalizedName = computed(() => normalizePlaylistName(name.value))
const canSubmit = computed(() => normalizedName.value.length > 0 && !submitting.value)

const focusInput = (): void => {
  void nextTick(() => nameInput.value?.focus())
}
focusInput()

const submit = async (): Promise<void> => {
  if (!canSubmit.value) return

  submitting.value = true
  errorMessage.value = ''

  try {
    const res = (await window.api.playlist_create({
      name: normalizedName.value,
      // 10 = 隐私歌单，0 = 公开歌单
      privacy: isPrivate.value ? 10 : 0,
      cookie: userStore.cookie
    })) as ServiceResult<{ playlist?: { id?: number } }>

    if (!isApiSuccess(res)) {
      errorMessage.value = apiErrorMessage(res, '创建歌单失败，请重试')
      return
    }

    const playlistId = res.body?.playlist?.id
    if (typeof playlistId !== 'number') {
      errorMessage.value = '创建歌单失败：服务端未返回歌单信息'
      return
    }

    emit('created', playlistId)
  } catch (error) {
    console.error('创建歌单失败', error)
    errorMessage.value = '创建歌单失败，请重试'
  } finally {
    submitting.value = false
  }
}
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop" @click.self="emit('close')">
      <div class="create-playlist-card glass-morphism-heavy">
        <div class="card-header">
          <div class="header-content">
            <span class="title">新建歌单</span>
            <span class="subtitle">创建后可在侧边栏直接找到</span>
          </div>
          <button class="close-btn" type="button" @click="emit('close')">
            <AppIcon name="close" :size="20" />
          </button>
        </div>

        <form class="card-body" @submit.prevent="submit">
          <label class="field">
            <span class="field-label">歌单名称</span>
            <input
              ref="nameInput"
              v-model="name"
              class="name-input"
              type="text"
              :maxlength="MAX_PLAYLIST_NAME_LENGTH"
              placeholder="比如「深夜循环」"
              :disabled="submitting"
              @keydown.esc="emit('close')"
            />
          </label>

          <label class="privacy-row">
            <span class="privacy-text">
              <span class="field-label">私密歌单</span>
              <span class="privacy-hint">只有自己可见，不会出现在个人主页</span>
            </span>
            <input v-model="isPrivate" class="switch" type="checkbox" :disabled="submitting" />
          </label>

          <div v-if="errorMessage" class="error-msg">{{ errorMessage }}</div>

          <div class="card-actions">
            <button class="btn btn-secondary" type="button" @click="emit('close')">取消</button>
            <button class="btn btn-primary" type="submit" :disabled="!canSubmit">
              <span v-if="submitting" class="btn-spinner"></span>
              {{ submitting ? '创建中...' : '创建' }}
            </button>
          </div>
        </form>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.3);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
}

.create-playlist-card {
  width: 380px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  user-select: none;
  -webkit-app-region: no-drag;
}

.glass-morphism-heavy {
  background: var(--glass-fill);
  backdrop-filter: var(--sys-glass-blur);
  -webkit-backdrop-filter: var(--sys-glass-blur);
  border: var(--glass-border-width-hairline) solid var(--glass-border);
  box-shadow:
    var(--glass-shadow-elevated),
    inset 0 2px 1px var(--glass-highlight),
    inset 0 -1.5px 1px var(--glass-shade),
    var(--glass-glow);
  border-radius: 28px;
}

.card-header {
  padding: 24px 24px 16px;
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
}

.title {
  font-size: 20px;
  font-weight: 800;
  color: var(--sys-text);
  display: block;
}

.subtitle {
  font-size: 12px;
  color: var(--sys-text-tertiary);
  font-weight: 600;
  margin-top: 2px;
  display: block;
}

.close-btn {
  background: var(--sys-control);
  border: none;
  width: 36px;
  height: 36px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: var(--sys-text-secondary);
  flex-shrink: 0;
  transition: all 0.2s;
}

.close-btn:hover {
  background: var(--sys-control-hover);
  color: var(--sys-text);
}

.card-body {
  padding: 0 24px 20px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.field-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--sys-text-secondary);
}

.name-input {
  width: 100%;
  height: var(--sys-control-height);
  padding: 0 14px;
  border-radius: 14px;
  border: 1px solid var(--sys-border);
  background: var(--sys-control);
  color: var(--sys-text);
  font-size: 14px;
  outline: none;
  transition:
    border-color 0.2s,
    background-color 0.2s;
}

.name-input::placeholder {
  color: var(--sys-text-disabled);
}

.name-input:focus {
  border-color: var(--theme-color);
  background: var(--sys-control-hover);
}

.privacy-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  cursor: pointer;
}

.privacy-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.privacy-hint {
  font-size: 11px;
  color: var(--sys-text-tertiary);
}

/* 开关复用设置页的现代 switch 观感，只画两个状态 */
.switch {
  appearance: none;
  width: 44px;
  height: 26px;
  border-radius: 100px;
  background: var(--sys-control-hover);
  position: relative;
  cursor: pointer;
  flex-shrink: 0;
  transition: background-color 0.2s;
  border: none;
}

.switch::after {
  content: '';
  position: absolute;
  top: 3px;
  left: 3px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--sys-surface);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  transition: transform 0.2s;
}

.switch:checked {
  background: var(--theme-color);
}

.switch:checked::after {
  transform: translateX(18px);
}

.switch:disabled {
  opacity: 0.5;
  cursor: default;
}

.error-msg {
  padding: 10px 14px;
  border-radius: 12px;
  font-size: 13px;
  font-weight: 600;
  background: var(--sys-danger-soft);
  color: var(--sys-danger);
}

.card-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  margin-top: 4px;
}

.btn {
  height: 40px;
  padding: 0 20px;
  border-radius: 100px;
  border: 1px solid transparent;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  transition:
    opacity 0.2s,
    background-color 0.2s;
}

.btn-secondary {
  background: var(--sys-control);
  border-color: var(--sys-border);
  color: var(--sys-text-secondary);
}

.btn-secondary:hover {
  background: var(--sys-control-hover);
  color: var(--sys-text);
}

.btn-primary {
  background: var(--theme-color);
  color: #fff;
}

.btn-primary:disabled {
  opacity: 0.45;
  cursor: default;
}

.btn-spinner {
  width: 14px;
  height: 14px;
  border: 2px solid rgba(255, 255, 255, 0.4);
  border-top-color: #fff;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
