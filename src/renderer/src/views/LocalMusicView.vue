<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import { computed, onBeforeUnmount, ref } from 'vue'
import MediaDetailLayout from '@renderer/components/media/MediaDetailLayout.vue'
import SongList from '@renderer/components/media/SongList.vue'
import { useDialogStore } from '@renderer/stores/dialogStore'
import { useLocalMusicStore } from '@renderer/stores/localMusicStore'
import { formatCurrentSongArtists, usePlayerStore } from '@renderer/stores/playerStore'
import type { LocalSong } from '@renderer/types/player'
import type { Song } from '@renderer/types/songDetail'

const localMusicStore = useLocalMusicStore()
const playerStore = usePlayerStore()
const dialogStore = useDialogStore()

const fileInput = ref<HTMLInputElement | null>(null)
const searchQuery = ref('')
const importMessage = ref('')
const isDraggingOver = ref(false)
let messageTimer: number | undefined

const filteredLocalSongs = computed(() => {
  const query = searchQuery.value.trim().toLocaleLowerCase()
  if (!query) return localMusicStore.songs

  return localMusicStore.songs.filter((song) => {
    return [song.name, song.fileName, song.album, formatCurrentSongArtists(song.artists)].some(
      (value) => value?.toLocaleLowerCase().includes(query)
    )
  })
})

const mapLocalSongToSong = (song: LocalSong): Song => ({
  id: song.id,
  name: song.name,
  dt: song.duration,
  ar: song.artists,
  al: {
    id: 0,
    name: song.fileName,
    picUrl: song.cover
  }
})

const displaySongs = computed(() => filteredLocalSongs.value.map(mapLocalSongToSong))
const pageDescription = computed(
  () => importMessage.value || '保存在此设备上的音乐，导入记录会保留在本地曲库中。'
)

const showImportMessage = (message: string): void => {
  importMessage.value = message
  if (messageTimer) window.clearTimeout(messageTimer)
  messageTimer = window.setTimeout(() => {
    importMessage.value = ''
  }, 4_000)
}

const importFiles = async (files: File[]): Promise<void> => {
  if (!files.length || localMusicStore.isImporting) return
  const result = await localMusicStore.importFiles(files)
  const messages: string[] = []
  if (result.imported) messages.push(`已导入 ${result.imported} 首`)
  if (result.skipped) messages.push(`已跳过 ${result.skipped} 个`)
  if (result.failed.length) messages.push(`${result.failed.length} 个导入失败`)
  showImportMessage(messages.join('，') || '没有可导入的音频文件')
}

const openFilePicker = (): void => {
  fileInput.value?.click()
}

const handleFileSelection = (event: Event): void => {
  const input = event.target as HTMLInputElement
  void importFiles(Array.from(input.files ?? []))
  input.value = ''
}

const handleDrop = (event: DragEvent): void => {
  isDraggingOver.value = false
  void importFiles(Array.from(event.dataTransfer?.files ?? []))
}

const playAll = (): void => {
  if (!filteredLocalSongs.value.length) return
  void playerStore.playAll(filteredLocalSongs.value)
}

const playSong = (song: Song): void => {
  const startIndex = filteredLocalSongs.value.findIndex((item) => item.id === song.id)
  if (startIndex < 0) return
  void playerStore.playAll(filteredLocalSongs.value, startIndex)
}

const removeSong = (id: number): void => {
  localMusicStore.removeSong(id)
  playerStore.playlist = playerStore.playlist.filter((song) => song.id !== id)
}

const clearLibrary = async (): Promise<void> => {
  const confirmed = await dialogStore.open({
    title: '清空本地音乐',
    message: '将所有歌曲移出应用曲库，但不会删除磁盘上的音乐文件。',
    mode: 'confirm-cancel',
    confirmText: '清空'
  })
  if (!confirmed) return

  const localIds = new Set(localMusicStore.songs.map((song) => song.id))
  localMusicStore.clear()
  playerStore.playlist = playerStore.playlist.filter((song) => !localIds.has(song.id))
}

onBeforeUnmount(() => {
  if (messageTimer) window.clearTimeout(messageTimer)
})
</script>

<template>
  <div
    class="local-music-view"
    :class="{ 'is-dragging-over': isDraggingOver }"
    @dragenter.prevent="isDraggingOver = true"
    @dragover.prevent="isDraggingOver = true"
    @dragleave.self="isDraggingOver = false"
    @drop.prevent="handleDrop"
  >
    <input
      ref="fileInput"
      class="file-input"
      type="file"
      accept="audio/*,.aac,.aif,.aiff,.ape,.flac,.m4a,.mp3,.ogg,.opus,.wav,.webm,.wma"
      multiple
      @change="handleFileSelection"
    />

    <MediaDetailLayout
      v-model:search-query="searchQuery"
      :loading="false"
      :title="'本地音乐'"
      :description="pageDescription"
      :meta="[`${localMusicStore.songCount} 首歌曲`]"
      :play-disabled="!filteredLocalSongs.length"
      :show-more="false"
      search-placeholder="在本地音乐中搜索..."
      @play-all="playAll"
    >
      <template #cover>
        <div class="local-library-cover" aria-hidden="true">
          <AppIcon name="music" :size="24" />
        </div>
      </template>

      <template #actions>
        <button
          class="local-action-button import-button"
          type="button"
          :disabled="localMusicStore.isImporting"
          @click="openFilePicker"
        >
          <AppIcon name="download" :size="20" />
          {{ localMusicStore.isImporting ? '正在导入' : '导入音乐' }}
        </button>
        <button
          v-if="localMusicStore.songCount"
          class="local-icon-button"
          type="button"
          title="清空本地曲库"
          aria-label="清空本地曲库"
          @click="clearLibrary"
        >
          <AppIcon name="trash-outline" :size="18" />
        </button>
      </template>

      <SongList
        :songs="displaySongs"
        :search-query="searchQuery"
        variant="local"
        @play="playSong"
        @removed="removeSong"
      />
    </MediaDetailLayout>
  </div>
</template>

<style scoped>
.local-music-view {
  height: 100%;
  min-width: 0;
  transition: box-shadow 0.18s ease;
}

.local-music-view.is-dragging-over {
  box-shadow: inset 0 0 0 2px var(--theme-color);
}

.file-input {
  display: none;
}

.local-library-cover {
  width: 100%;
  height: 100%;
  display: grid;
  place-items: center;
  background: var(--sys-control);
  color: var(--theme-color-strong);
}

.local-library-cover svg {
  width: 92px;
  height: 92px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.25;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.local-action-button,
.local-icon-button {
  height: 44px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--sys-border);
  background: var(--sys-control);
  color: var(--sys-text-secondary);
  cursor: pointer;
  transition:
    background 0.2s,
    color 0.2s;
}

.local-action-button {
  gap: 7px;
  padding: 0 16px;
  border-radius: 100px;
  font-size: 13px;
  font-weight: 600;
}

.local-icon-button {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  transition:
    background 0.2s,
    border-color 0.2s,
    color 0.2s,
    transform 0.2s;
}

.local-action-button svg,
.local-icon-button svg {
  width: 18px;
  height: 18px;
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.local-action-button:hover,
.local-icon-button:hover {
  background: var(--sys-control-hover);
  color: var(--sys-text);
}

.local-action-button:disabled {
  cursor: default;
  opacity: 0.5;
}

.local-icon-button:hover {
  background: var(--sys-danger-soft);
  border-color: var(--sys-danger);
  color: var(--sys-danger);
}

.local-icon-button:active {
  transform: scale(0.94);
}

.local-icon-button:focus-visible {
  outline: 2px solid var(--theme-color);
  outline-offset: 2px;
}
</style>
