export interface AudioDeviceInfo {
  id: string
  name: string
  isDefault: boolean
  isCurrent: boolean
  /** 同一端点的全部 id 写法（含 id 本身；虚拟端点只有自身）。数字卡号写法
   * 在 USB 拔插后会漂移，持久化的设备身份与激活匹配都要经别名表比较，
   * 避免“设备在却显示不可用/写法分裂后每次开播都重复切换”。 */
  aliases?: string[]
}

/** 端点身份匹配：id 本身或任一别名命中即视为同一设备。 */
export function isSameAudioDevice(
  device: Pick<AudioDeviceInfo, 'id' | 'aliases'> | null | undefined,
  deviceId: string | null | undefined
): boolean {
  if (!device || typeof deviceId !== 'string') return false
  if (device.id === deviceId) return true
  return device.aliases?.includes(deviceId) ?? false
}
