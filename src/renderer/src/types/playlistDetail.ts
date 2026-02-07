/**
 * 响应体内容
 */
export interface PlaylistDetail {
  code: number;
  relatedVideos: unknown | null;
  playlist: Playlist;
  urls: unknown | null;
  privileges: Privilege[];
  sharedPrivilege: unknown | null;
  resEntrance: unknown | null;
  fromUsers: unknown | null;
  fromUserCount: number;
  songFromUsers: unknown | null;
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
  updateFrequency: unknown | null;
  backgroundCoverId: number;
  backgroundCoverUrl: string | null;
  titleImage: number;
  titleImageUrl: string | null;
  detailPageTitle: string | null;
  englishTitle: string | null;
  officialPlaylistType: unknown | null;
  copied: boolean;
  relateResType: unknown | null;
  coverStatus: number;
  subscribers: unknown[];
  subscribed: unknown | null;
  creator: Creator;
  tracks: Track[];
  trackIds: TrackIdInfo[];
  videoIds: unknown | null;
  videos: unknown | null;
  shareCount: number;
  commentCount: number;
  remixVideo: unknown | null;
  newDetailPageRemixVideo: unknown | null;
  sharedUsers: unknown | null;
  historySharedUsers: unknown | null;
  gradeStatus: string;
  score: unknown | null;
  algTags: unknown | null;
  distributeTags: unknown[];
  trialMode: number;
  displayTags: unknown | null;
  displayUserInfoAsTagOnly: boolean;
  playlistType: string;
  bizExtInfo: Record<string, unknown>;
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
  experts: unknown | null;
  djStatus: number;
  vipType: number;
  remarkName: string | null;
  authenticationTypes: number;
  avatarDetail: unknown | null;
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
  crbt: unknown | null;
  cf: string;
  al: Album;
  dt: number;
  h: AudioQuality | null;
  m: AudioQuality | null;
  l: AudioQuality | null;
  sq: AudioQuality | null;
  hr: AudioQuality | null;
  a: unknown | null;
  cd: string;
  no: number;
  rtUrl: unknown | null;
  ftype: number;
  rtUrls: unknown[];
  djId: number;
  copyright: number;
  s_id: number;
  mark: number;
  originCoverType: number;
  originSongSimpleData: OriginSongData | null;
  tagPicList: unknown | null;
  resourceState: boolean;
  version: number;
  songJumpInfo: unknown | null;
  entertainmentTags: unknown | null;
  awardTags: unknown | null;
  displayTags: unknown | null;
  single: number;
  noCopyrightRcmd: unknown | null;
  alg: unknown | null;
  displayReason: unknown | null;
  rtype: number;
  rurl: unknown | null;
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
  sc: unknown | null;
  f: unknown | null;
  sr: unknown | null;
  dpr: unknown | null;
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
  pc: unknown | null;
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
  rscl: unknown | null;
  freeTrialPrivilege: FreeTrialPrivilege;
  rightSource: number;
  chargeInfoList: ChargeInfo[];
  code: number;
  message: string | null;
}

export interface FreeTrialPrivilege {
  resConsumable: boolean;
  userConsumable: boolean;
  listenType: unknown | null;
  cannotListenReason: unknown | null;
  playReason: unknown | null;
  freeLimitTagType: unknown | null;
}

export interface ChargeInfo {
  rate: number;
  chargeUrl: string | null;
  chargeMessage: string | null;
  chargeType: number;
}
