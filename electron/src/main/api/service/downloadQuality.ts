import { DOWNLOAD_QUALITIES, type DownloadQualityOption } from '../../../shared/download'

export interface DownloadPrivilege {
  id: number
  dl?: number
  dlLevel?: string
  dlLevels?: string[] | null
  downloadMaxBrLevel?: string
  maxBrLevel?: string
  downloadMaxbr?: number
  maxbr?: number
  fee?: number
  st?: number
  code?: number
}

export interface DownloadSongDetail {
  id: number
  l?: { br?: number } | null
  h?: { br?: number } | null
  sq?: { br?: number } | null
  hr?: { br?: number } | null
}

// higher 是接口可能返回的 192 kbps 档位；本应用不提供该选项，但需要正确比较上限。
const QUALITY_LEVELS = [
  'none',
  'standard',
  'higher',
  'exhigh',
  'lossless',
  'hires',
  'jyeffect',
  'sky',
  'jymaster'
]

function levelRank(level: string | undefined): number | undefined {
  if (!level) return undefined
  const rank = QUALITY_LEVELS.indexOf(level)
  return rank >= 0 ? rank : undefined
}

function bitrateRank(bitrate: number | undefined): number | undefined {
  if (typeof bitrate !== 'number' || !Number.isFinite(bitrate)) return undefined
  if (bitrate <= 0) return 0
  if (bitrate >= 999000) return QUALITY_LEVELS.indexOf('lossless')
  if (bitrate >= 320000) return QUALITY_LEVELS.indexOf('exhigh')
  if (bitrate >= 192000) return QUALITY_LEVELS.indexOf('higher')
  if (bitrate >= 128000) return QUALITY_LEVELS.indexOf('standard')
  return 0
}

export function getDownloadQualityOptions(
  song: DownloadSongDetail,
  privilege: DownloadPrivilege
): DownloadQualityOption[] {
  // dlLevel/dl 是携带当前 cookie 后的下载权限，不能拿音源上限或试听权限代替。
  const allowedLevels = Array.isArray(privilege.dlLevels) ? privilege.dlLevels : undefined
  const accountRank = levelRank(privilege.dlLevel) ?? bitrateRank(privilege.dl)
  if (!allowedLevels && accountRank === undefined) {
    throw new Error('未能获取当前账号的下载权限，请重试')
  }
  const sourceRank =
    levelRank(privilege.downloadMaxBrLevel) ??
    levelRank(privilege.maxBrLevel) ??
    // 999000 只是无损资源的哨兵值，不能据此认定歌曲没有 Hi-Res 或母带。
    ((privilege.downloadMaxbr ?? privilege.maxbr ?? 0) >= 999000
      ? undefined
      : bitrateRank(privilege.downloadMaxbr ?? privilege.maxbr))
  const sourceFields = { standard: 'l', exhigh: 'h', lossless: 'sq', hires: 'hr' } as const
  const unavailable = (privilege.st ?? 0) < 0 || (privilege.code ?? 0) >= 400

  return DOWNLOAD_QUALITIES.map(({ quality }) => {
    const rank = QUALITY_LEVELS.indexOf(quality)
    let reason: string | undefined
    const accountAllows = allowedLevels ? allowedLevels.includes(quality) : rank <= accountRank!
    const sourceField = sourceFields[quality as keyof typeof sourceFields]
    if (unavailable) {
      reason = '歌曲暂不可下载'
    } else if (!accountAllows) {
      reason = privilege.fee === 4 ? '需购买专辑或单曲' : '当前会员或购买权限不支持'
    } else if (
      (sourceRank !== undefined && rank > sourceRank) ||
      (sourceField && song[sourceField] === null)
    ) {
      reason = '歌曲暂无此音质'
    }
    return { quality, available: !reason, ...(reason ? { reason } : {}) }
  })
}
