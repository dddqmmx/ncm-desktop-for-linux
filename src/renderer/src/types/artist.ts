export interface ArtistSecondaryIdentity {
  expertIdentiyId: number
  expertIdentiyName: string
  expertIdentiyCount: number
}

export interface ArtistProfile {
  id: number
  name: string
  cover?: string
  avatar?: string
  alias: string[]
  transNames: string[]
  identities: string[]
  identifyTag?: string[] | null
  briefDesc: string
  albumSize: number
  musicSize: number
  mvSize: number
}

export interface ArtistDetailData {
  videoCount?: number
  identify?: {
    imageUrl?: string | null
    imageDesc?: string | null
    actionUrl?: string
  }
  artist?: ArtistProfile
  secondaryExpertIdentiy?: ArtistSecondaryIdentity[]
}

export interface ArtistDetailResponse {
  code: number
  message?: string
  data?: ArtistDetailData
}

export interface ArtistRef {
  id: number
  name: string
}

export interface ArtistAlbumOwner {
  id: number
  name: string
  alias?: string[]
  picUrl?: string
  img1v1Url?: string
  albumSize?: number
  musicSize?: number
}

export interface ArtistAlbum {
  id: number
  name: string
  picUrl?: string
  publishTime: number
  size: number
  type?: string
  subType?: string
  company?: string
  artist?: ArtistRef
  artists?: ArtistRef[]
}

export interface ArtistAlbumResponse {
  artist?: ArtistAlbumOwner
  hotAlbums?: ArtistAlbum[]
}

export interface ArtistSongAlbum {
  id: number
  name: string
  picUrl?: string
}

export interface ArtistSong {
  id: number
  name: string
  dt: number
  mv: number
  pop?: number
  publishTime?: number
  alia?: string[]
  ar: ArtistRef[]
  al: ArtistSongAlbum
}

export interface ArtistTopSongResponse {
  code: number
  more?: boolean
  songs?: ArtistSong[]
}

export interface ArtistMv {
  id: number
  name: string
  artistName: string
  imgurl16v9?: string
  imgurl?: string
  duration: number
  playCount: number
  publishTime?: string
  status?: number
}

export interface ArtistMvResponse {
  time?: number
  hasMore?: boolean
  mvs?: ArtistMv[]
}
