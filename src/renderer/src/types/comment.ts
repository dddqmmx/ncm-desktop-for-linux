export interface CommentUser {
  userId: number
  nickname: string
  avatarUrl: string
  vipType?: number
}

export interface CommentReference {
  user: CommentUser | null
  content: string | null
}

export interface SongComment {
  commentId: number
  content: string
  /** 毫秒时间戳 */
  time: number
  /** 接口直接给出的日期文本，如 2020-01-05 */
  timeStr?: string
  likedCount: number
  user: CommentUser | null
  /** 被回复的评论（楼中楼引用），内容可能已被删除 */
  beReplied?: CommentReference[]
}

export interface SongCommentsBody {
  total: number
  more: boolean
  hotComments?: SongComment[]
  comments: SongComment[]
}
