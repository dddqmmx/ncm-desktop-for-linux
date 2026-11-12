export interface LyricUser {
  id: number
  status: number
  demand: number
  userid: number
  nickname: string
  uptime: number
}

export interface LyricBlock {
  version: number
  lyric: string
}

export interface Lyric {
  sgc: boolean
  sfy: boolean
  qfy: boolean
  lyricUser?: LyricUser
  lrc?: LyricBlock
  klyric?: LyricBlock
  tlyric?: LyricBlock
  romalrc?: LyricBlock
  code: number
}
