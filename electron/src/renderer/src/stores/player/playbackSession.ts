/**
 * 播放会话身份：哪首歌真正装载在音频引擎里。
 *
 * 存在的理由是把「界面显示的歌」和「引擎里装载的歌」分开。开播流程在真正开播
 * **之前**就会把 UI 的 currentSongId 乐观切到目标歌（loading 期间列表要立即
 * 高亮），isPlaying 也在同一时刻被置为 false。于是任何「currentSongId 已经是
 * 这首歌、且此刻不在播放 ⇒ 目标歌只是暂停了」的推断，在请求还在途中时同样成立，
 * 会把**上一首**的会话当成目标歌的暂停会话去 resume——快速连点「播放全部」时
 * 界面切了歌、声音还是上一首，就是这个推断的直接后果。
 *
 * 这里只回答两个问题，其余状态（isPlaying / isLoading）不参与判断：
 * 1. 这次播放请求是否仍然是最新的（`isCurrent` / `commit`）；
 * 2. 是否有一份「装载的正是这首歌、且没有新请求在途」的会话可以直接恢复
 *    （`canResume`）。
 *
 * 不变式：
 * - `loadedSongId` 只由 `commit` 写入成功的请求，任何地方都不许自己猜。
 * - `begin` 之后、`commit`/`clear` 之前，`canResume` 恒为 false——在途请求随时
 *   会替换旧会话，拿它 resume 就是把上一首续上。
 * - 「最后一次点击生效」：`begin` 作废此前所有请求，过期请求的 `commit` 是
 *   无操作，不会把状态写回。
 * - `clear`（停止/失败/会话终结）同时作废在途请求。
 *
 * 可观察状态（`isBusy` / `loadedSongId`）用 Vue ref 承载：store 里派生的
 * isSwitching / isHistorySong 直接读它们，因此不存在「改了会话状态却忘了同步
 * 某个布尔标志」的窗口——这层同步正是本次故障的成因。
 */

import { ref } from 'vue'

export interface PlayAttempt {
  readonly songId: number
  readonly generation: number
}

export class PlaybackSessionTracker {
  private generation = 0
  private readonly pending = ref<PlayAttempt | null>(null)
  private readonly loaded = ref<number | null>(null)

  /** 开始一次播放请求。此前进行中的请求立即作废。 */
  begin(songId: number): PlayAttempt {
    this.generation += 1
    const attempt: PlayAttempt = { songId, generation: this.generation }
    this.pending.value = attempt
    return attempt
  }

  /** 是否有播放请求正在途中（UI 的「切换中」）。 */
  get isBusy(): boolean {
    return this.pending.value !== null
  }

  /** 引擎里真正装载着的那首歌。`null` = 没有可恢复的会话（历史回放/已停止）。 */
  get loadedSongId(): number | null {
    return this.loaded.value
  }

  /** 该请求是否仍是当前请求。 */
  isCurrent(attempt: PlayAttempt): boolean {
    return attempt.generation === this.generation
  }

  /**
   * 提交一次成功的开播：仍是最新请求时记为「已装载」并返回 true。
   * 返回 false 表示这次请求已被后来的请求取代（或已被 `clear` 作废），
   * 调用方必须放弃后续的状态写入——不要在 await 之后自己比较 token。
   */
  commit(attempt: PlayAttempt): boolean {
    if (!this.isCurrent(attempt)) return false
    this.pending.value = null
    this.loaded.value = attempt.songId
    return true
  }

  /** 显式停止 / 会话终结 / 播放失败：引擎里不再有可恢复的会话，并作废在途请求。 */
  clear(): void {
    this.generation += 1
    this.pending.value = null
    this.loaded.value = null
  }

  /**
   * 是否可以直接恢复已装载的会话，而不是重新开播。
   * 两个条件缺一不可：装载的正是这首歌；没有新请求在途。
   */
  canResume(songId: number): boolean {
    return this.pending.value === null && this.loaded.value === songId
  }
}
