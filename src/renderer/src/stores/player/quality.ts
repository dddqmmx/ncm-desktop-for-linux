import { SoundQualityType } from '@renderer/types/song'

const SOUND_QUALITY_LEVELS: SoundQualityType[] = [
  'jymaster', // 超清母带
  'sky', // 沉浸声
  'jyeffect', // 高清杜比
  'hires', // Hi-Res
  'lossless', // 无损
  'exhigh', // 极高 (320k)
  'standard' // 标准 (128k)
]

export const isSoundQualityLevel = (value: unknown): value is SoundQualityType => {
  return typeof value === 'string' && SOUND_QUALITY_LEVELS.includes(value as SoundQualityType)
}
