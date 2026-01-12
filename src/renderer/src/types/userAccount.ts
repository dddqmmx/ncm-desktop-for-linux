export interface UserAccount {
  code: number
  account: {
    id: number
    userName: string
    type: number
    status: number
    whitelistAuthority: number
    createTime: number
    tokenVersion: number
    ban: number
    baoyueVersion: number
    donateVersion: number
    vipType: number
    anonimousUser: boolean
    paidFee: boolean
  }
  profile: {
    userId: number
    userType: number
    nickname: string
    avatarImgId: number
    avatarUrl: string
    backgroundImgId: number
    backgroundUrl: string
    signature: string
    createTime: number
    userName: string
    accountType: number
    shortUserName: string
    birthday: number
    authority: number
    gender: number
    accountStatus: number
    province: number
    city: number
    authStatus: number
    description: string | null
    detailDescription: string | null
    defaultAvatar: boolean
    expertTags: any[] | null
    experts: any | null
    djStatus: number
    locationStatus: number
    vipType: number
    followed: boolean
    mutual: boolean
    authenticated: boolean
    lastLoginTime: number
    lastLoginIP: string
    remarkName: string | null
    viptypeVersion: number
    authenticationTypes: number
    avatarDetail: any | null
    anchor: boolean
  }
}
