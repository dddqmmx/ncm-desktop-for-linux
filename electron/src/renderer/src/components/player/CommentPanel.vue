<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import LazyImage from '@renderer/components/common/LazyImage.vue'
import { useUserStore } from '@renderer/stores/userStore'
import type { SongComment, SongCommentsBody } from '@renderer/types/comment'
import { formatCommentCount, formatCommentTime } from '@renderer/utils/comment'
import { computed, onBeforeUnmount, ref, watch } from 'vue'

const PAGE_SIZE = 20
// 距底部多远开始预取下一页，留一屏手感
const LOAD_MORE_THRESHOLD_PX = 240

const props = defineProps<{
  songId?: number
  isDark: boolean
}>()

const userStore = useUserStore()

const hotComments = ref<SongComment[]>([])
const comments = ref<SongComment[]>([])
const hasMore = ref(false)
const loading = ref(false)
const loadingMore = ref(false)
const failed = ref(false)
// 加载更多失败：保留 hasMore，底部给出重试入口而不是谎报"没有更多"
const loadMoreFailed = ref(false)
const scrollRef = ref<HTMLElement | null>(null)

// 跨页去重：热门评论与最新评论可能指向同一条，:key 用 commentId
const seenCommentIds = new Set<number>()
// 最新评论接口已取到的条数（不能拿去重后的列表长度当 offset）
const nextOffset = ref(0)
// 切歌会作废在途请求：token 不一致的响应直接丢弃
let requestToken = 0

const hasAnyComment = computed(() => hotComments.value.length > 0 || comments.value.length > 0)

const themeVars = computed(() => {
  const { isDark } = props
  return {
    '--cmt-text': isDark ? 'rgba(255, 255, 255, 0.92)' : 'rgba(0, 0, 0, 0.9)',
    '--cmt-text-secondary': isDark ? 'rgba(255, 255, 255, 0.62)' : 'rgba(0, 0, 0, 0.6)',
    '--cmt-text-tertiary': isDark ? 'rgba(255, 255, 255, 0.42)' : 'rgba(0, 0, 0, 0.4)',
    '--cmt-surface': isDark ? 'rgba(255, 255, 255, 0.07)' : 'rgba(0, 0, 0, 0.045)',
    '--cmt-surface-hover': isDark ? 'rgba(255, 255, 255, 0.12)' : 'rgba(0, 0, 0, 0.08)',
    '--cmt-border': isDark ? 'rgba(255, 255, 255, 0.14)' : 'rgba(0, 0, 0, 0.1)',
    '--cmt-divider': isDark ? 'rgba(255, 255, 255, 0.08)' : 'rgba(0, 0, 0, 0.06)',
    '--cmt-scrollbar': isDark ? 'rgba(255, 255, 255, 0.22)' : 'rgba(0, 0, 0, 0.18)'
  }
})

function dedupe(list: SongComment[]): SongComment[] {
  const result: SongComment[] = []
  for (const comment of list) {
    if (typeof comment?.commentId !== 'number' || seenCommentIds.has(comment.commentId)) continue
    seenCommentIds.add(comment.commentId)
    result.push(comment)
  }
  return result
}

function resetComments(): void {
  hotComments.value = []
  comments.value = []
  hasMore.value = false
  failed.value = false
  loadMoreFailed.value = false
  nextOffset.value = 0
  seenCommentIds.clear()
}

async function fetchComments(offset: number, append: boolean): Promise<void> {
  const songId = props.songId
  if (!songId) return

  const token = ++requestToken
  if (append) {
    loadingMore.value = true
    loadMoreFailed.value = false
  } else {
    loading.value = true
    resetComments()
  }

  try {
    const res = (await window.api.comment_music({
      id: songId,
      limit: PAGE_SIZE,
      offset,
      cookie: userStore.cookie
    })) as { body?: SongCommentsBody }
    if (token !== requestToken) return

    const body = res.body
    if (!body) throw new Error('empty comment response')

    const page = body.comments ?? []
    hasMore.value = Boolean(body.more)
    if (!append) {
      hotComments.value = dedupe(body.hotComments ?? [])
    }
    const fresh = dedupe(page)
    comments.value = append ? [...comments.value, ...fresh] : fresh
    nextOffset.value = offset + page.length
    // 接口偶尔返回 more=true 却给不出新条目，避免无限预取
    if (fresh.length === 0) hasMore.value = false
  } catch {
    if (token !== requestToken) return
    if (append) {
      loadMoreFailed.value = true
    } else {
      failed.value = true
    }
  } finally {
    if (token === requestToken) {
      loading.value = false
      loadingMore.value = false
    }
  }
}

function loadMore(): void {
  if (!hasMore.value || loading.value || loadingMore.value) return
  void fetchComments(nextOffset.value, true)
}

function handleScroll(): void {
  // 失败后不再自动重试：停在底部时的轻微滚动会无限重放同一个失败请求，
  // 交给底部的重试按钮显式触发
  if (loadMoreFailed.value) return
  const container = scrollRef.value
  if (!container) return
  const distanceToBottom = container.scrollHeight - container.scrollTop - container.clientHeight
  if (distanceToBottom <= LOAD_MORE_THRESHOLD_PX) loadMore()
}

function retry(): void {
  void fetchComments(0, false)
}

interface CommentSection {
  label: string
  items: CommentView[]
}

interface CommentView {
  id: number
  nickname: string
  avatar: string | undefined
  isVip: boolean
  time: string
  content: string
  reply: { nickname: string; content: string } | null
  liked: string
}

// 模板里不做取值/回退判断：渲染前一次性把原始评论摊平成视图模型，
// 楼中楼引用也只解析一次。
function toView(comment: SongComment): CommentView {
  const reply = comment.beReplied?.find((item) => item?.content)
  return {
    id: comment.commentId,
    nickname: comment.user?.nickname || '已注销用户',
    avatar: comment.user?.avatarUrl,
    isVip: Boolean(comment.user?.vipType),
    time: formatCommentTime(comment),
    content: comment.content,
    reply: reply?.content
      ? { nickname: reply.user?.nickname ?? '已注销用户', content: reply.content }
      : null,
    liked: formatCommentCount(comment.likedCount) || '赞'
  }
}

const sections = computed<CommentSection[]>(() => {
  const result: CommentSection[] = []
  if (hotComments.value.length > 0) {
    result.push({ label: '热门评论', items: hotComments.value.map(toView) })
  }
  if (comments.value.length > 0) {
    result.push({ label: '最新评论', items: comments.value.map(toView) })
  }
  return result
})

watch(
  () => props.songId,
  (songId) => {
    // 切歌先作废在途请求，再把列表清空，避免旧歌评论闪现在新歌下
    requestToken++
    resetComments()
    loading.value = false
    loadingMore.value = false
    loadMoreFailed.value = false
    scrollRef.value?.scrollTo({ top: 0, behavior: 'auto' })
    if (songId) void fetchComments(0, false)
  },
  { immediate: true }
)

onBeforeUnmount(() => {
  requestToken++
})
</script>

<template>
  <section class="comment-panel" :style="themeVars">
    <div v-if="loading" class="panel-status">
      <span class="spinner"></span>
      加载中...
    </div>

    <div v-else-if="failed" class="panel-status">
      <span>评论加载失败</span>
      <button type="button" class="retry-btn" @click="retry">重试</button>
    </div>

    <div v-else-if="!hasAnyComment" class="panel-status">暂无评论</div>

    <div v-else ref="scrollRef" class="comment-scroll" @scroll.passive="handleScroll">
      <template v-for="section in sections" :key="section.label">
        <div class="section-label">{{ section.label }}</div>
        <article v-for="item in section.items" :key="item.id" class="comment-item">
          <div class="comment-avatar">
            <LazyImage :id="item.avatar" type="avatar" param="100y100" rounded />
          </div>
          <div class="comment-main">
            <div class="comment-meta">
              <span class="comment-nickname">{{ item.nickname }}</span>
              <span v-if="item.isVip" class="comment-vip">VIP</span>
              <span class="comment-time">{{ item.time }}</span>
            </div>
            <p class="comment-content">{{ item.content }}</p>
            <blockquote v-if="item.reply" class="comment-reply">
              <span class="reply-nickname">@{{ item.reply.nickname }}：</span>
              <span class="reply-content">{{ item.reply.content }}</span>
            </blockquote>
            <div class="comment-footer">
              <AppIcon name="like" :size="13" class="like-icon" />
              <span>{{ item.liked }}</span>
            </div>
          </div>
        </article>
      </template>

      <div class="list-footer">
        <template v-if="loadingMore">
          <span class="spinner"></span>
          加载中...
        </template>
        <template v-else-if="loadMoreFailed">
          <button type="button" class="more-btn" @click="loadMore">加载失败，点击重试</button>
        </template>
        <template v-else-if="hasMore">
          <button type="button" class="more-btn" @click="loadMore">加载更多</button>
        </template>
        <template v-else>没有更多评论了</template>
      </div>
    </div>
  </section>
</template>

<style scoped>
.comment-panel {
  position: relative;
  height: 100%;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: var(--cmt-text);
  /* 全屏页根元素是窗口拖拽区，列表与按钮必须豁免才能滚动/点击 */
  -webkit-app-region: no-drag;
}

.comment-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 4px 6px 20px 2px;
  scrollbar-gutter: stable;
  overscroll-behavior: contain;
  scrollbar-width: thin;
  scrollbar-color: var(--cmt-scrollbar) transparent;
}

.comment-scroll::-webkit-scrollbar {
  width: 6px;
}

.comment-scroll::-webkit-scrollbar-thumb {
  background-color: var(--cmt-scrollbar);
  border-radius: 3px;
}

.section-label {
  font-size: 12px;
  font-weight: 700;
  color: var(--cmt-text-tertiary);
  padding: 6px 2px;
  margin-top: 6px;
}

.comment-item {
  display: flex;
  gap: 12px;
  padding: 14px 0;
  border-bottom: 1px solid var(--cmt-divider);
}

.comment-avatar {
  width: 36px;
  height: 36px;
  flex-shrink: 0;
  border-radius: 50%;
  overflow: hidden;
}

.comment-main {
  flex: 1;
  min-width: 0;
}

.comment-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.comment-nickname {
  font-size: 13px;
  font-weight: 600;
  color: var(--cmt-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.comment-vip {
  flex-shrink: 0;
  font-size: 9px;
  font-weight: 800;
  letter-spacing: 0.04em;
  padding: 1px 5px;
  border-radius: 6px;
  border: 1px solid var(--cmt-border);
  color: var(--cmt-text-tertiary);
}

.comment-time {
  flex-shrink: 0;
  margin-left: auto;
  font-size: 11px;
  color: var(--cmt-text-tertiary);
  font-variant-numeric: tabular-nums;
}

.comment-content {
  margin: 6px 0 0;
  font-size: 14px;
  line-height: 1.55;
  color: var(--cmt-text);
  white-space: pre-wrap;
  word-break: break-word;
}

.comment-reply {
  margin: 8px 0 0;
  padding: 8px 10px;
  border-radius: 10px;
  background: var(--cmt-surface);
  font-size: 12.5px;
  line-height: 1.5;
  color: var(--cmt-text-secondary);
  white-space: pre-wrap;
  word-break: break-word;
}

.reply-nickname {
  color: var(--cmt-text);
  font-weight: 600;
}

.reply-content {
  white-space: pre-wrap;
  word-break: break-word;
}

.comment-footer {
  display: flex;
  align-items: center;
  gap: 5px;
  margin-top: 8px;
  font-size: 11px;
  color: var(--cmt-text-tertiary);
  font-variant-numeric: tabular-nums;
}

.like-icon {
  opacity: 0.7;
}

.panel-status {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  font-size: 13px;
  color: var(--cmt-text-tertiary);
}

.retry-btn {
  padding: 6px 18px;
  border-radius: 999px;
  border: 1px solid var(--cmt-border);
  background: var(--cmt-surface);
  color: var(--cmt-text-secondary);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition:
    background 0.2s ease,
    color 0.2s ease;
}

.retry-btn:hover {
  background: var(--cmt-surface-hover);
  color: var(--cmt-text);
}

.list-footer {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 18px 0 8px;
  font-size: 12px;
  color: var(--cmt-text-tertiary);
}

.more-btn {
  padding: 6px 18px;
  border-radius: 999px;
  border: 1px solid var(--cmt-border);
  background: var(--cmt-surface);
  color: var(--cmt-text-secondary);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition:
    background 0.2s ease,
    color 0.2s ease;
}

.more-btn:hover {
  background: var(--cmt-surface-hover);
  color: var(--cmt-text);
}

.spinner {
  width: 16px;
  height: 16px;
  border: 2px solid var(--cmt-border);
  border-top-color: var(--cmt-text-secondary);
  border-radius: 50%;
  animation: comment-spin 0.8s linear infinite;
}

@keyframes comment-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
