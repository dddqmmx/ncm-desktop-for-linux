export interface PlaylistCatlist {
  code: number
  all: PlaylistCategory
  sub: PlaylistCategory[]
  categories: Record<number, string>
}

export interface PlaylistCategory {
  name: string
  resourceCount: number
  imgId: number
  imgUrl: string | null
  type: number
  category: number
  resourceType: number
  hot: boolean
  activity: boolean
}
