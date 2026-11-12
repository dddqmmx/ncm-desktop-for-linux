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
  source?: 'local'
  filePath?: string
  fileName?: string
  album?: string
}

export interface PersistedCurrentSong {
  id?: number
  name?: string
  artist?: CurrentSongArtist[] | CurrentSongArtist | string | null
  artists?: CurrentSongArtist[] | CurrentSongArtist | string | null
  cover?: string
  duration?: number
  source?: 'local'
  filePath?: string
  fileName?: string
  album?: string
}

export interface LocalSong extends CurrentSong {
  source: 'local'
  filePath: string
  fileName: string
}

export const isLocalSong = (song: CurrentSong | null | undefined): song is LocalSong => {
  return song?.source === 'local' && typeof song.filePath === 'string' && song.filePath.length > 0
}

export interface Privilege {
  id: number
  playMaxBrLevel: string // jymaster, lossless, exhigh 等
  chargeInfoList: unknown[]
}
