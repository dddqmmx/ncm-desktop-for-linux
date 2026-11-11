import { describe, expect, it } from 'vitest'
import { MAX_PLAYLIST_NAME_LENGTH, normalizePlaylistName } from '../playlistName'

describe('normalizePlaylistName', () => {
  it('trims surrounding whitespace', () => {
    expect(normalizePlaylistName('  深夜循环  ')).toBe('深夜循环')
  })

  it('rejects empty or whitespace-only names', () => {
    expect(normalizePlaylistName('')).toBe('')
    expect(normalizePlaylistName('   ')).toBe('')
    expect(normalizePlaylistName('\n\t')).toBe('')
  })

  it('collapses inner whitespace runs so the sidebar stays readable', () => {
    expect(normalizePlaylistName('深夜    循环')).toBe('深夜 循环')
  })

  it('caps the length at the limit', () => {
    const long = '歌'.repeat(MAX_PLAYLIST_NAME_LENGTH + 20)
    expect(normalizePlaylistName(long)).toHaveLength(MAX_PLAYLIST_NAME_LENGTH)
  })

  it('leaves a valid name untouched', () => {
    expect(normalizePlaylistName('American Classic')).toBe('American Classic')
  })
})
