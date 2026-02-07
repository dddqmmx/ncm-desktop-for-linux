export interface RecommendSongs {
  code: number;
  data: DailySongsData;
}

/**
 * 主数据内容
 */
export interface DailySongsData {
  fromCache: boolean;
  dailySongs: Song[];
  orderSongs: unknown[];
  recommendReasons: RecommendReason[];
  mvResourceInfos: unknown | null;
  demote: boolean;
  algReturnDemote: boolean;
  dailyRecommendInfo: unknown | null;
}

/**
 * 歌曲详细信息
 */
export interface Song {
  name: string;
  mainTitle: string | null;
  additionalTitle: string | null;
  id: number;
  pst: number;
  t: number;
  ar: Artist[];           // 歌手列表
  alia: string[];         // 别名
  pop: number;            // 人气值
  st: number;
  rt: string | null;
  fee: number;            // 费用标识
  v: number;              // 版本
  crbt: unknown | null;
  cf: string;
  al: Album;              // 专辑信息
  dt: number;             // 时长 (ms)
  h: AudioQuality;        // 高品质音源
  m: AudioQuality;        // 中品质音源
  l: AudioQuality;        // 低品质音源
  sq: AudioQuality | null; // 无损音源
  hr: AudioQuality | null; // Hi-Res音源
  a: unknown | null;
  cd: string;             // CD编号
  no: number;             // 歌曲序号
  rtUrl: string | null;
  ftype: number;
  rtUrls: unknown[];
  djId: number;
  copyright: number;
  s_id: number;
  mark: number;
  originCoverType: number;
  originSongSimpleData: unknown | null;
  tagPicList: unknown | null;
  resourceState: boolean;
  version: number;
  songJumpInfo: unknown | null;
  entertainmentTags: unknown | null;
  awardTags: unknown | null;
  displayTags: unknown | null;
  markTags: unknown[];
  single: number;
  noCopyrightRcmd: unknown | null;
  rtype: number;
  rurl: string | null;
  mst: number;
  cp: number;
  mv: number;             // MV ID
  publishTime: number;    // 发布时间戳
  reason: string | null;  // 推荐理由
  tns?: string[];         // 翻译名 (某些歌曲存在)
  recommendReason: string | null;
  privilege: Privilege;   // 权限详情
  alg: string;            // 推荐算法标识
}

/**
 * 歌手信息
 */
export interface Artist {
  id: number;
  name: string;
  tns: string[];
  alias: string[];
}

/**
 * 专辑信息
 */
export interface Album {
  id: number;
  name: string;
  picUrl: string;
  tns: string[];
  pic_str?: string;
  pic: number;
}

/**
 * 音质信息
 */
export interface AudioQuality {
  br: number;   // 比特率
  fid: number;
  size: number; // 文件大小
  vd: number;
  sr: number;   // 采样率
}

/**
 * 歌曲播放权限与付费信息
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
  plLevels: string[] | null;
  dlLevels: string[] | null;
  ignoreCache: boolean | null;
  bd: unknown | null;
}

/**
 * 免费试听权限
 */
export interface FreeTrialPrivilege {
  resConsumable: boolean;
  userConsumable: boolean;
  listenType: unknown | null;
  cannotListenReason: unknown | null;
  playReason: unknown | null;
  freeLimitTagType: unknown | null;
}

/**
 * 付费方案列表
 */
export interface ChargeInfo {
  rate: number;
  chargeUrl: string | null;
  chargeMessage: string | null;
  chargeType: number;
}

/**
 * 推荐理由摘要
 */
export interface RecommendReason {
  songId: number;
  reason: string;
  reasonId: string;
  targetUrl: string | null;
}
