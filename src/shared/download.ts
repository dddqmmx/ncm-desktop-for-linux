export const DOWNLOAD_QUALITIES = [
  { quality: 'standard', label: '标准', description: '128 kbps' },
  { quality: 'exhigh', label: '极高', description: '320 kbps' },
  { quality: 'lossless', label: '无损', description: 'FLAC' },
  { quality: 'hires', label: 'Hi-Res', description: '高解析度' },
  { quality: 'jyeffect', label: '高清臻音', description: '' },
  { quality: 'sky', label: '沉浸环绕声', description: '' },
  { quality: 'jymaster', label: '超清母带', description: '' }
] as const

export type DownloadQuality = (typeof DOWNLOAD_QUALITIES)[number]['quality']

export const isDownloadQuality = (value: unknown): value is DownloadQuality =>
  DOWNLOAD_QUALITIES.some((option) => option.quality === value)

export interface SongDownloadOptionsRequest {
  songId: number
  cookie?: string
}

export interface DownloadQualityOption {
  quality: DownloadQuality
  available: boolean
  reason?: string
}

export function preferredDownloadQuality(
  options: DownloadQualityOption[],
  preferred: DownloadQuality
): DownloadQuality | null {
  const available = DOWNLOAD_QUALITIES.filter(({ quality }) =>
    options.some((option) => option.quality === quality && option.available)
  )
  const preferredIndex = DOWNLOAD_QUALITIES.findIndex((option) => option.quality === preferred)
  return (
    available
      .filter(
        ({ quality }) =>
          DOWNLOAD_QUALITIES.findIndex((option) => option.quality === quality) <= preferredIndex
      )
      .at(-1)?.quality ??
    available.at(-1)?.quality ??
    null
  )
}

export type SongDownloadOptionsResult =
  | { status: 'ready'; options: DownloadQualityOption[] }
  | { status: 'error'; message: string }

export interface SongDownloadRequest {
  songId: number
  songName: string
  artists: string
  quality: DownloadQuality
  cookie?: string
}

export type SongDownloadResult =
  | { status: 'completed'; filePath: string; quality: DownloadQuality }
  | { status: 'cancelled' }
  | { status: 'error'; message: string }
