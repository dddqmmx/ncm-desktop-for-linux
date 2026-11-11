import type { SongComment } from '@renderer/types/comment'

/** 评论数/点赞数：过万才压缩，便于在窄列里显示 */
export function formatCommentCount(count: number): string {
  if (!Number.isFinite(count) || count <= 0) return ''
  if (count >= 100_000_000) return `${(count / 100_000_000).toFixed(1)}亿`
  if (count >= 10_000) return `${(count / 10_000).toFixed(1)}万`
  return String(Math.round(count))
}

/** 接口自带 timeStr 时直接使用，否则把毫秒时间戳格式化为本地日期 */
export function formatCommentTime(comment: Pick<SongComment, 'time' | 'timeStr'>): string {
  if (comment.timeStr) return comment.timeStr
  if (!Number.isFinite(comment.time) || comment.time <= 0) return ''

  const date = new Date(comment.time)
  if (Number.isNaN(date.getTime())) return ''

  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${date.getFullYear()}-${month}-${day}`
}
