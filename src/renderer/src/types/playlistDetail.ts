/**
 * 响应体内容
 */
export interface PlaylistDetail {
  code: number;
  relatedVideos: any | null;
  playlist: Playlist;
  urls: any | null;
  privileges: Privilege[];
  sharedPrivilege: any | null;
  resEntrance: any | null;
  fromUsers: any | null;
  fromUserCount: number;
  songFromUsers: any | null;
}

/**
 * 歌单详情
 */
export interface Playlist {
  id: number;
  name: string;
  coverImgId: number;
  coverImgUrl: string;
  coverImgId_str: string;
  adType: number;
  userId: number;
  createTime: number;
  status: number;
  opRecommend: boolean;
  highQuality: boolean;
  newImported: boolean;
  updateTime: number;
  trackCount: number;
  specialType: number;
  privacy: number;
  trackUpdateTime: number;
  commentThreadId: string;
  playCount: number;
  trackNumberUpdateTime: number;
  subscribedCount: number;
  cloudTrackCount: number;
  ordered: boolean;
  description: string | null;
  tags: string[];
  updateFrequency: any | null;
  backgroundCoverId: number;
  backgroundCoverUrl: string | null;
  titleImage: number;
  titleImageUrl: string | null;
  detailPageTitle: string | null;
  englishTitle: string | null;
  officialPlaylistType: any | null;
  copied: boolean;
  relateResType: any | null;
  coverStatus: number;
  subscribers: any[];
  subscribed: any | null;
  creator: Creator;
  tracks: Track[];
  trackIds: TrackIdInfo[];
  videoIds: any | null;
  videos: any | null;
  shareCount: number;
  commentCount: number;
  remixVideo: any | null;
  newDetailPageRemixVideo: any | null;
  sharedUsers: any | null;
  historySharedUsers: any | null;
  gradeStatus: string;
  score: any | null;
  algTags: any | null;
  distributeTags: any[];
  trialMode: number;
  displayTags: any | null;
  displayUserInfoAsTagOnly: boolean;
  playlistType: string;
  bizExtInfo: Record<string, any>;
  mixPodcastPlaylist: boolean;
  podcastTrackCount: number;
}

/**
 * 歌单创建者信息
 */
export interface Creator {
  defaultAvatar: boolean;
  province: number;
  authStatus: number;
  followed: boolean;
  avatarUrl: string;
  accountStatus: number;
  gender: number;
  city: number;
  birthday: number;
  userId: number;
  userType: number;
  nickname: string;
  signature: string;
  description: string;
  detailDescription: string;
  avatarImgId: number;
  backgroundImgId: number;
  backgroundUrl: string;
  authority: number;
  mutual: boolean;
  expertTags: string[] | null;
  experts: any | null;
  djStatus: number;
  vipType: number;
  remarkName: string | null;
  authenticationTypes: number;
  avatarDetail: any | null;
  avatarImgIdStr: string;
  backgroundImgIdStr: string;
  anchor: boolean;
  avatarImgId_str: string;
}

/**
 * 歌曲详细信息
 */
export interface Track {
  name: string;
  id: number;
  pst: number;
  t: number;
  ar: Artist[];
  alia: string[];
  pop: number;
  st: number;
  rt: string | null;
  fee: number;
  v: number;
  crbt: any | null;
  cf: string;
  al: Album;
  dt: number;
  h: AudioQuality | null;
  m: AudioQuality | null;
  l: AudioQuality | null;
  sq: AudioQuality | null;
  hr: AudioQuality | null;
  a: any | null;
  cd: string;
  no: number;
  rtUrl: any | null;
  ftype: number;
  rtUrls: any[];
  djId: number;
  copyright: number;
  s_id: number;
  mark: number;
  originCoverType: number;
  originSongSimpleData: OriginSongData | null;
  tagPicList: any | null;
  resourceState: boolean;
  version: number;
  songJumpInfo: any | null;
  entertainmentTags: any | null;
  awardTags: any | null;
  displayTags: any | null;
  single: number;
  noCopyrightRcmd: any | null;
  alg: any | null;
  displayReason: any | null;
  rtype: number;
  rurl: any | null;
  mst: number;
  cp: number;
  mv: number;
  publishTime: number;
  tns?: string[]; // 翻译名，可选
  mainTitle?: string | null;
  additionalTitle?: string | null;
}

export interface Artist {
  id: number;
  name: string;
  tns: string[];
  alias: string[];
}

export interface Album {
  id: number;
  name: string;
  picUrl: string;
  tns: string[];
  pic_str?: string;
  pic: number;
}

export interface AudioQuality {
  br: number;
  fid: number;
  size: number;
  vd: number;
  sr: number;
}

/**
 * 歌曲来源的简单数据（如原唱信息）
 */
export interface OriginSongData {
  songId: number;
  name: string;
  artists: { id: number; name: string }[];
  albumMeta: { id: number; name: string };
}

/**
 * 歌单中的歌曲 ID 列表项
 */
export interface TrackIdInfo {
  id: number;
  v: number;
  t: number;
  at: number;
  alg: string | null;
  uid: number;
  rcmdReason: string;
  rcmdReasonTitle: string;
  sc: any | null;
  f: any | null;
  sr: any | null;
  dpr: any | null;
  tr: number;
}

/**
 * 歌曲权限信息
 */
export interface Privilege {
  id: number;
  fee: number;
  payed: number;
  realPayed: number;
  st: number;
  pl: number;
  dl: number;
  sp: number;
  cp: number;
  subp: number;
  cs: boolean;
  maxbr: number;
  fl: number;
  pc: any | null;
  toast: boolean;
  flag: number;
  paidBigBang: boolean;
  preSell: boolean;
  playMaxbr: number;
  downloadMaxbr: number;
  maxBrLevel: string;
  playMaxBrLevel: string;
  downloadMaxBrLevel: string;
  plLevel: string;
  dlLevel: string;
  flLevel: string;
  rscl: any | null;
  freeTrialPrivilege: FreeTrialPrivilege;
  rightSource: number;
  chargeInfoList: ChargeInfo[];
  code: number;
  message: string | null;
}

export interface FreeTrialPrivilege {
  resConsumable: boolean;
  userConsumable: boolean;
  listenType: any | null;
  cannotListenReason: any | null;
  playReason: any | null;
  freeLimitTagType: any | null;
}

export interface ChargeInfo {
  rate: number;
  chargeUrl: string | null;
  chargeMessage: string | null;
  chargeType: number;
}
