import { Song } from '@renderer/types/songDetail'
import { SoundQualityType } from '@renderer/types/song'
import { Privilege } from '@renderer/types/player'

// 降级顺序（从高到低）
export const DOWNGRADE_ORDER: SoundQualityType[] = [
  'jymaster', // 超清母带
  'sky', // 沉浸声
  'jyeffect', // 高清杜比
  'hires', // Hi-Res
  'lossless', // 无损
  'exhigh', // 极高 (320k)
  'standard' // 标准 (128k)
]

/**
 * 获取当前歌曲支持的所有音质列表
 */
export const getAvailableQualities = (song: Song, privilege: Privilege): SoundQualityType[] => {
  const available: SoundQualityType[] = ['standard'] // 默认支持标准

  // 1. 检查旧版字段 (存在即代表有资源)
  if (song.h) available.push('exhigh')
  if (song.sq) available.push('lossless')
  if (song.hr) available.push('hires')

  // 2. 检查新版权限字段 (Privilege)
  const level = privilege.playMaxBrLevel

  if (level === 'jymaster') {
    available.push('jymaster')
    // 通常支持 master 也意味着支持以下音质，补全以防万一
    if (!available.includes('lossless')) available.push('lossless')
    if (!available.includes('exhigh')) available.push('exhigh')
  }

  if (level === 'sky') available.push('sky')
  if (level === 'jyeffect') available.push('jyeffect')

  // 去重
  return Array.from(new Set(available))
}

/**
 * 根据目标音质计算最终请求的音质 (Level)
 */
export const computePlayableLevel = (
  available: SoundQualityType[],
  targetQuality: SoundQualityType
): SoundQualityType => {
  // 如果目标音质直接可用
  if (available.includes(targetQuality)) {
    return targetQuality
  }

  // 否则按降级顺序查找
  const targetIndex = DOWNGRADE_ORDER.indexOf(targetQuality)
  // 从目标音质的下一级开始找
  for (let i = targetIndex + 1; i < DOWNGRADE_ORDER.length; i++) {
    const quality = DOWNGRADE_ORDER[i]
    if (available.includes(quality)) {
      console.log(`[音质降级] 目标: ${targetQuality} -> 实际: ${quality}`)
      return quality
    }
  }

  return 'standard' // 兜底
}
