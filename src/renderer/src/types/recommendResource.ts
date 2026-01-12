export interface RecommendResource {
  code: number
  featureFirst: boolean
  haveRcmdSongs: boolean
  recommend: RecommendItem[]
}

export interface RecommendItem {
  id: number
  type: number
  name: string
  copywriter: string
  picUrl: string
  playcount: number
  createTime: number
  creator: Creator
  trackCount: number
  userId: number
  alg: string
}

export interface Creator {
  avatarImgIdStr: string
  backgroundImgIdStr: string
  city: number
  vipType: number
  province: number
  birthday: number
  accountStatus: number
  avatarUrl: string
  authStatus: number
  userType: number
  nickname: string
  gender: number
  backgroundUrl: string
  avatarImgId: number
  backgroundImgId: number
  detailDescription: string
  defaultAvatar: boolean
  expertTags: string[] | null
  djStatus: number
  followed: boolean
  mutual: boolean
  remarkName: string | null
  description: string
  userId: number
  signature: string
  authority: number
}
