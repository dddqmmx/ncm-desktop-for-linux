import { describe, expect, it } from 'vitest'
import { formatCommentCount, formatCommentTime } from '../comment'

describe('comment formatting', () => {
  it('keeps small counts as-is and compresses counts above ten thousand', () => {
    expect(formatCommentCount(0)).toBe('')
    expect(formatCommentCount(41)).toBe('41')
    expect(formatCommentCount(225023)).toBe('22.5万')
    expect(formatCommentCount(123456789)).toBe('1.2亿')
  })

  it('prefers the date text supplied by the API', () => {
    expect(formatCommentTime({ time: 1578232920291, timeStr: '2020-01-05' })).toBe('2020-01-05')
  })

  it('formats a millisecond timestamp as a local date when timeStr is missing', () => {
    const time = new Date(2020, 0, 5, 12, 30).getTime()
    expect(formatCommentTime({ time })).toBe('2020-01-05')
  })

  it('returns an empty string for unusable timestamps', () => {
    expect(formatCommentTime({ time: 0 })).toBe('')
    expect(formatCommentTime({ time: Number.NaN })).toBe('')
  })
})
