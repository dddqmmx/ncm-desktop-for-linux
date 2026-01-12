export type SoundQualityType =
  | 'standard'
  | 'exhigh'
  | 'lossless'
  | 'hires'
  | 'jyeffect'
  | 'sky'
  | 'jymaster'

  export interface SongUrl {
  id: number;
  url: string;
  br: number;
  size: number;
  md5: string;
  code: number;
  expi: number;
  type: string;
  gain: number;
  peak: number;
  closedGain: number;
  closedPeak: number;
  fee: number;
  uf: null;
  payed: number;
  flag: number;
  canExtend: boolean;
  freeTrialInfo: null;
  level: string;
  encodeType: string;
  channelLayout: null;
  freeTrialPrivilege: FreeTrialPrivilege;
  freeTimeTrialPrivilege: FreeTimeTrialPrivilege;
  urlSource: number;
  rightSource: number;
  podcastCtrp: null;
  effectTypes: null;
  time: number;
  message: null;
  levelConfuse: null;
  musicId: string;
  accompany: null;
  sr: number;
  auEff: null;
  immerseType: null;
  beatType: number;
}

export interface FreeTrialPrivilege {
  resConsumable: boolean;
  userConsumable: boolean;
  listenType: null;
  cannotListenReason: null;
  playReason: null;
  freeLimitTagType: null;
}

export interface FreeTimeTrialPrivilege {
  resConsumable: boolean;
  userConsumable: boolean;
  type: number;
  remainTime: number;
}
