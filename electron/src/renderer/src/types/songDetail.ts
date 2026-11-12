interface LooseObject {
  [key: string]: unknown
}

export interface Artist {
  id: number
  name: string
  tns?: string[]
  alias?: string[]
}

export interface Album {
  id: number
  name: string
  picUrl: string
  pic?: number
  pic_str?: string
  tns?: string[]
}

export interface Quality {
  br: number
  size: number
  sr: number
  vd?: number
  fid?: number
}

export interface Song extends LooseObject {
  id: number
  name: string
  dt: number
  ar: Artist[]
  al: Album

  h?: Quality | null
  m?: Quality | null
  l?: Quality | null

  mv?: number
  fee?: number
  publishTime?: number
}

export interface SongPrivilege extends LooseObject {
  id: number
  fee: number
  pl: number
  dl: number
  maxbr: number
  playMaxbr: number
  downloadMaxbr: number
  code: number
}

export interface SongDetailResult {
  songs: Song[]
  privileges: SongPrivilege[]
  code: number
}
