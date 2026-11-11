import { describe, expect, it } from 'vitest'
import {
  DOWNLOAD_QUALITIES,
  preferredDownloadQuality,
  type DownloadQualityOption,
  type DownloadQuality
} from '../../../../shared/download'
import { getDownloadQualityOptions, type DownloadPrivilege } from '../downloadQuality'

const song = { id: 42 }
const optionsFor = (privilege: Partial<DownloadPrivilege>): DownloadQualityOption[] =>
  getDownloadQualityOptions(song, { id: 42, ...privilege })
const availableFor = (privilege: Partial<DownloadPrivilege>): DownloadQuality[] =>
  optionsFor(privilege)
    .filter((option) => option.available)
    .map((option) => option.quality)

describe('account-specific download quality permissions', () => {
  it.each([
    ['standard', ['standard']],
    ['exhigh', ['standard', 'exhigh']],
    ['lossless', ['standard', 'exhigh', 'lossless']],
    ['hires', ['standard', 'exhigh', 'lossless', 'hires']],
    ['jymaster', DOWNLOAD_QUALITIES.map((option) => option.quality)]
  ])('uses the current account download ceiling %s', (dlLevel, available) => {
    expect(availableFor({ dlLevel: dlLevel as string, downloadMaxBrLevel: 'jymaster' })).toEqual(
      available
    )
  })

  it('does not grant membership-only downloads from the catalog quality ceiling', () => {
    expect(
      availableFor({
        dl: 0,
        dlLevel: 'none',
        downloadMaxBrLevel: 'jymaster',
        maxbr: 999000,
        fee: 1
      })
    ).toEqual([])
    expect(
      optionsFor({ dlLevel: 'standard' }).find((item) => item.quality === 'lossless')?.reason
    ).toContain('权限')
  })

  it('honors free-song or purchased-song rights without a fixed nonmember quality cap', () => {
    expect(availableFor({ dlLevel: 'hires', fee: 0 })).toContain('hires')
    expect(availableFor({ dlLevel: 'hires', fee: 4 })).toContain('hires')
    expect(optionsFor({ dlLevel: 'none', fee: 4 })[0].reason).toContain('购买')
  })

  it('allows only explicitly granted levels when the server supplies a level list', () => {
    expect(
      availableFor({ dlLevel: 'jymaster', dlLevels: ['standard', 'lossless', 'sky'] })
    ).toEqual(['standard', 'lossless', 'sky'])
    expect(availableFor({ dlLevel: 'jymaster', dlLevels: [] })).toEqual([])
  })

  it('does not offer missing audio sources even for accounts with higher privileges', () => {
    expect(availableFor({ dlLevel: 'jymaster', downloadMaxBrLevel: 'exhigh' })).toEqual([
      'standard',
      'exhigh'
    ])
    const options = getDownloadQualityOptions({ id: 42, hr: null }, { id: 42, dlLevel: 'hires' })
    expect(options.find((item) => item.quality === 'hires')).toMatchObject({
      available: false,
      reason: '歌曲暂无此音质'
    })
  })

  it.each([
    [128000, ['standard']],
    [192000, ['standard']],
    [320000, ['standard', 'exhigh']],
    [999000, ['standard', 'exhigh', 'lossless']],
    [0, []]
  ])('falls back conservatively to the account download bitrate %i', (dl, available) => {
    expect(availableFor({ dl: dl as number })).toEqual(available)
  })

  it('does not interpret the lossless resource sentinel as a Hi-Res restriction', () => {
    expect(availableFor({ dlLevel: 'hires', downloadMaxbr: 999000 })).toContain('hires')
  })

  it('rejects missing permissions and unavailable songs', () => {
    expect(() => availableFor({ maxBrLevel: 'jymaster' })).toThrow('下载权限')
    expect(availableFor({ dlLevel: 'jymaster', st: -1 })).toEqual([])
  })

  it('selects the preferred quality or the nearest available lower quality', () => {
    const options = optionsFor({ dlLevel: 'lossless' })
    expect(preferredDownloadQuality(options, 'standard')).toBe('standard')
    expect(preferredDownloadQuality(options, 'hires')).toBe('lossless')
    expect(preferredDownloadQuality(optionsFor({ dlLevel: 'none' }), 'hires')).toBeNull()
  })
})
