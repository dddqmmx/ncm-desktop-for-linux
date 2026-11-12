import { describe, expect, it } from 'vitest'
import { PlaybackSessionTracker } from '../player/playbackSession'

describe('PlaybackSessionTracker', () => {
  it('reports no resumable session until a request actually commits', () => {
    const session = new PlaybackSessionTracker()

    const attempt = session.begin(1)
    // 请求在途：即便 UI 已经显示这首歌，也没有可恢复的会话
    expect(session.canResume(1)).toBe(false)
    expect(session.loadedSongId).toBeNull()
    expect(session.isBusy).toBe(true)

    expect(session.commit(attempt)).toBe(true)
    expect(session.canResume(1)).toBe(true)
    expect(session.isBusy).toBe(false)
  })

  it('invalidates the previous request when a newer one begins', () => {
    const session = new PlaybackSessionTracker()
    const first = session.begin(1)
    const second = session.begin(2)

    expect(session.isCurrent(first)).toBe(false)
    expect(session.isCurrent(second)).toBe(true)
    // 上一次请求的提交不得写回装载身份
    expect(session.commit(first)).toBe(false)
    expect(session.loadedSongId).toBeNull()
    expect(session.commit(second)).toBe(true)
    expect(session.loadedSongId).toBe(2)
  })

  it('never lets a request in flight be resumed over the still-loaded session', () => {
    const session = new PlaybackSessionTracker()
    const first = session.begin(1)
    session.commit(first)

    // 切歌请求已开始，但目标歌尚未装载：此刻能把旧会话续上才是 bug
    session.begin(2)
    expect(session.canResume(1)).toBe(false)
    expect(session.canResume(2)).toBe(false)
    // 装载身份仍是旧歌，UI 据此知道该走"重新开播"而不是"恢复"
    expect(session.loadedSongId).toBe(1)
  })

  it('drops the resumable session and in-flight requests on clear', () => {
    const session = new PlaybackSessionTracker()
    const attempt = session.begin(1)
    session.commit(attempt)

    session.clear()
    expect(session.loadedSongId).toBeNull()
    expect(session.canResume(1)).toBe(false)
    expect(session.isBusy).toBe(false)

    // clear 之前发起的请求也一并作废
    const stale = session.begin(3)
    session.clear()
    expect(session.isCurrent(stale)).toBe(false)
    expect(session.commit(stale)).toBe(false)
  })
})
