export interface PlaylistResponse {
  more: boolean
  playlist: Playlist[]
  code: number
}

export interface Playlist {
  id: number
  name: string
  description: string | null
  coverImgUrl: string
  coverImgId: number
  coverImgId_str: string
  trackCount: number
  playCount: number
  subscribedCount: number
  cloudTrackCount: number
  userId: number
  createTime: number
  updateTime: number
  trackUpdateTime: number
  trackNumberUpdateTime: number
  commentThreadId: string
  privacy: number
  status: number
  ordered: boolean
  highQuality: boolean
  anonimous: boolean
  newImported: boolean
  specialType: number
  adType: number
  totalDuration: number
  tags: string[]
  subscribers: unknown[]
  subscribed: boolean | null
  sharedUsers: unknown[] | null
  shareStatus: unknown | null
  copied: boolean
  containsTracks: boolean
  top: boolean
  updateFrequency: string | null
  backgroundCoverId: number
  backgroundCoverUrl: string | null
  titleImage: number
  titleImageUrl: string | null
  englishTitle: string | null
  opRecommend: boolean
  recommendInfo: unknown | null
  artists: unknown[] | null
  tracks: unknown[] | null
  creator: User
}

export interface User {
  userId: number
  nickname: string
  avatarUrl: string
  defaultAvatar: boolean
  province: number
  city: number
  gender: number
  birthday: number
  signature: string
  description: string
  detailDescription: string
  accountStatus: number
  authStatus: number
  authority: number
  userType: number
  vipType: number
  djStatus: number
  followed: boolean
  mutual: boolean
  anchor: boolean
  expertTags: string[] | null
  experts: unknown | null
  remarkName: string | null
  authenticationTypes: number
  avatarDetail: unknown | null
  avatarImgId: number
  avatarImgIdStr: string
  avatarImgId_str: string
  backgroundImgId: number
  backgroundImgIdStr: string
  backgroundUrl: string
}
