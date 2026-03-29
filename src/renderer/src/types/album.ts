import { Artist, Album, Song } from './songDetail'

export interface AlbumDetailInfo extends Album {
  artist: Artist
  artists: Artist[]
  description?: string
  publishTime: number
  size: number
  company?: string
  subType?: string
  type?: string
  onSale?: boolean
  tags?: string[]
  status?: number
  copyrightId?: number
  commentThreadId?: string
}

export interface AlbumDetail {
  album: AlbumDetailInfo
  songs: Song[]
  code: number
}
