export type PlayMode = 'loop' | 'random' | 'single'

export interface CurrentSongArtist {
  id: number
  name: string
}

export interface CurrentSong {
  id: number
  name: string
  artists: CurrentSongArtist[]
  cover: string
  duration: number
}

export interface PersistedCurrentSong {
  id?: number
  name?: string
  artist?: CurrentSongArtist[] | CurrentSongArtist | string | null
  artists?: CurrentSongArtist[] | CurrentSongArtist | string | null
  cover?: string
  duration?: number
}

export interface Privilege {
  id: number
  playMaxBrLevel: string // jymaster, lossless, exhigh 等
  chargeInfoList: unknown[]
}
