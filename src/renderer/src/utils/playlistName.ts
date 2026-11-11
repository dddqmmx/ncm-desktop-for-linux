/** 创建歌单时的名称上限，前后端（输入框 maxlength + 提交前校验）共用一份 */
export const MAX_PLAYLIST_NAME_LENGTH = 40

/**
 * 归一化用户输入的歌单名：
 * 去首尾空白、把中间连续空白压成一个空格、截断到上限。
 * 返回空串表示不可提交（纯空白/空名）。
 */
export function normalizePlaylistName(raw: string): string {
  const collapsed = raw.replace(/\s+/g, ' ').trim()
  return collapsed.slice(0, MAX_PLAYLIST_NAME_LENGTH)
}
