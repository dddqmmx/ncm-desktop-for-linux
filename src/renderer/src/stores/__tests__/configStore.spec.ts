import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useConfigStore } from '../configStore'
import type { AudioDeviceInfo } from '@renderer/types/audio'

class MemoryStorage implements Storage {
  private readonly store = new Map<string, string>()

  get length(): number {
    return this.store.size
  }

  clear(): void {
    this.store.clear()
  }

  getItem(key: string): string | null {
    return this.store.get(key) ?? null
  }

  key(index: number): string | null {
    return Array.from(this.store.keys())[index] ?? null
  }

  removeItem(key: string): void {
    this.store.delete(key)
  }

  setItem(key: string, value: string): void {
    this.store.set(key, value)
  }
}

type ApiMock = {
  get_output_devices: ReturnType<typeof vi.fn>
  switch_output_device: ReturnType<typeof vi.fn>
}

const STORAGE_KEY = 'app_settings'

function createDevice(id: string, overrides: Partial<AudioDeviceInfo> = {}): AudioDeviceInfo {
  return {
    id,
    name: id,
    isDefault: false,
    isCurrent: false,
    // 别名表默认只含 id 自身；跨写法测试显式传入
    aliases: [id],
    ...overrides
  }
}

function createApiMock(): ApiMock {
  return {
    get_output_devices: vi.fn(),
    switch_output_device: vi.fn()
  }
}

function readPersistedSettings(storage: Storage): Record<string, unknown> {
  return JSON.parse(storage.getItem(STORAGE_KEY) ?? '{}') as Record<string, unknown>
}

function writePersistedSettings(storage: Storage, settings: Record<string, unknown>): void {
  storage.setItem(STORAGE_KEY, JSON.stringify(settings))
}

describe('configStore output device handling', () => {
  let storage: MemoryStorage
  let api: ApiMock

  /** 设置持久化 watch 带 120ms 尾沿去抖，冲刷后才能读到 localStorage 里的值 */
  const flushPersistedSettings = async (): Promise<void> => {
    await nextTick()
    vi.advanceTimersByTime(150)
    await nextTick()
  }

  beforeEach(() => {
    storage = new MemoryStorage()
    api = createApiMock()

    vi.stubGlobal('localStorage', storage)
    vi.stubGlobal('window', {
      api
    } as unknown as Window & typeof globalThis)

    vi.spyOn(console, 'error').mockImplementation(() => undefined)
    vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    vi.useFakeTimers()
    setActivePinia(createPinia())
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('preserves the configured device in storage when runtime playback falls back to default', async () => {
    const preferredDeviceId = 'pulse'
    const preferredDeviceName = 'USB DAC'
    writePersistedSettings(storage, {
      outputDeviceId: preferredDeviceId,
      outputDeviceName: preferredDeviceName
    })

    api.get_output_devices.mockResolvedValue([
      createDevice('default', {
        name: 'System Default',
        isDefault: true,
        isCurrent: true
      })
    ])
    api.switch_output_device.mockImplementation(async (deviceId?: string) => {
      if (deviceId === preferredDeviceId) {
        throw new Error('device temporarily unavailable')
      }
    })

    const store = useConfigStore()
    const appliedDeviceId = await store.ensureConfiguredOutputDevice()
    await nextTick()

    expect(appliedDeviceId).toBe('default')
    expect(store.outputDeviceId).toBe(preferredDeviceId)
    expect(store.outputDeviceName).toBe(preferredDeviceName)
    expect(readPersistedSettings(storage).outputDeviceId).toBe(preferredDeviceId)
    expect(readPersistedSettings(storage).outputDeviceName).toBe(preferredDeviceName)
    expect(store.outputDevices.map((device) => device.id)).toContain(preferredDeviceId)
    expect(store.outputDevices.find((device) => device.id === preferredDeviceId)?.name).toContain(
      preferredDeviceName
    )
    expect(store.outputDevices.find((device) => device.id === preferredDeviceId)?.name).toContain(
      '当前不可用'
    )
    expect(api.switch_output_device.mock.calls).toEqual([
      [preferredDeviceId, false, false],
      [undefined, false, false]
    ])
  })

  it('does not recreate the device when the configured device is already current', async () => {
    const preferredDeviceId = 'pulse'
    writePersistedSettings(storage, {
      outputDeviceId: preferredDeviceId,
      outputDeviceName: 'USB DAC'
    })

    api.get_output_devices.mockResolvedValue([
      createDevice(preferredDeviceId, {
        name: 'USB DAC',
        isCurrent: true
      }),
      createDevice('default', {
        name: 'System Default',
        isDefault: true,
        isCurrent: false
      })
    ])

    const store = useConfigStore()

    expect(await store.ensureConfiguredOutputDevice()).toBe(preferredDeviceId)
    expect(api.switch_output_device).not.toHaveBeenCalled()
    expect(store.currentOutputDevice?.id).toBe(preferredDeviceId)
    expect(store.outputDeviceName).toBe('USB DAC')
  })

  it('keeps the configured device selectable even when the refreshed device list omits it', async () => {
    const preferredDeviceId = 'plughw:CARD=Device,DEV=0'
    const preferredDeviceName = 'External DAC'
    writePersistedSettings(storage, {
      outputDeviceId: preferredDeviceId,
      outputDeviceName: preferredDeviceName
    })

    api.get_output_devices.mockResolvedValue([
      createDevice('default', {
        name: 'System Default',
        isDefault: true,
        isCurrent: true
      })
    ])

    const store = useConfigStore()
    const devices = await store.refreshOutputDevices()

    expect(store.outputDeviceId).toBe(preferredDeviceId)
    expect(devices[0]).toMatchObject({
      id: preferredDeviceId,
      isDefault: false,
      isCurrent: false
    })
    expect(store.outputDeviceName).toBe(preferredDeviceName)
    expect(devices[0]?.name).toContain(preferredDeviceName)
    expect(devices[0]?.name).toContain('当前不可用')
  })

  it('backfills the stored device name after refreshing a legacy config that only saved the id', async () => {
    const preferredDeviceId = 'pulse'
    writePersistedSettings(storage, { outputDeviceId: preferredDeviceId })

    api.get_output_devices.mockResolvedValue([
      createDevice(preferredDeviceId, {
        name: 'USB DAC',
        isCurrent: true
      }),
      createDevice('default', {
        name: 'System Default',
        isDefault: true,
        isCurrent: false
      })
    ])

    const store = useConfigStore()
    await store.refreshOutputDevices()
    await flushPersistedSettings()

    expect(store.outputDeviceName).toBe('USB DAC')
    expect(readPersistedSettings(storage).outputDeviceName).toBe('USB DAC')
  })

  it('restores the configured device on the next successful ensure after a transient fallback', async () => {
    const preferredDeviceId = 'pulse'
    let currentDeviceId = 'default'
    let allowPreferredDevice = false

    writePersistedSettings(storage, {
      outputDeviceId: preferredDeviceId,
      outputDeviceName: 'USB DAC'
    })

    api.get_output_devices.mockImplementation(async () => {
      const devices = [
        createDevice('default', {
          name: 'System Default',
          isDefault: true,
          isCurrent: currentDeviceId === 'default'
        })
      ]

      if (allowPreferredDevice) {
        devices.push(
          createDevice(preferredDeviceId, {
            name: 'USB DAC',
            isCurrent: currentDeviceId === preferredDeviceId
          })
        )
      }

      return devices
    })
    api.switch_output_device.mockImplementation(async (deviceId?: string) => {
      const targetDeviceId = deviceId ?? 'default'

      if (targetDeviceId === preferredDeviceId && !allowPreferredDevice) {
        throw new Error('device busy')
      }

      currentDeviceId = targetDeviceId
    })

    const store = useConfigStore()

    expect(await store.ensureConfiguredOutputDevice()).toBe('default')

    allowPreferredDevice = true

    expect(await store.ensureConfiguredOutputDevice()).toBe(preferredDeviceId)
    await nextTick()

    expect(store.outputDeviceId).toBe(preferredDeviceId)
    expect(store.outputDeviceName).toBe('USB DAC')
    expect(readPersistedSettings(storage).outputDeviceId).toBe(preferredDeviceId)
    expect(readPersistedSettings(storage).outputDeviceName).toBe('USB DAC')
    expect(store.currentOutputDevice?.id).toBe(preferredDeviceId)
    expect(api.switch_output_device.mock.calls).toEqual([
      [preferredDeviceId, false, false],
      [undefined, false, false],
      [preferredDeviceId, false, false]
    ])
  })

  it('treats following-system-default as active when the list marks no current device', async () => {
    // Linux 硬件端点列表：跟随系统默认时没有任何设备带 is_current/is_default
    api.get_output_devices.mockResolvedValue([
      createDevice('hw:CARD=0,DEV=0', { name: 'HDA Intel PCH' })
    ])

    const store = useConfigStore()

    expect(await store.ensureConfiguredOutputDevice()).toBe('default')
    expect(api.switch_output_device).not.toHaveBeenCalled()
  })

  it('persists only explicit user selections, including switching back to default', async () => {
    const preferredDeviceId = 'hw:CARD=DAC,DEV=0'
    let currentDeviceId = 'default'

    api.get_output_devices.mockImplementation(async () => [
      createDevice(preferredDeviceId, {
        name: 'USB DAC',
        isCurrent: currentDeviceId === preferredDeviceId
      }),
      createDevice('default', {
        name: 'System Default',
        isDefault: true,
        isCurrent: currentDeviceId === 'default'
      })
    ])
    api.switch_output_device.mockImplementation(async (deviceId?: string) => {
      currentDeviceId = deviceId ?? 'default'
    })

    const store = useConfigStore()

    expect(await store.setOutputDevice(preferredDeviceId)).toBe(true)
    await flushPersistedSettings()
    expect(store.outputDeviceId).toBe(preferredDeviceId)
    expect(store.outputDeviceName).toBe('USB DAC')
    expect(readPersistedSettings(storage).outputDeviceId).toBe(preferredDeviceId)
    expect(readPersistedSettings(storage).outputDeviceName).toBe('USB DAC')

    expect(await store.setOutputDevice('default')).toBe(true)
    await flushPersistedSettings()
    expect(store.outputDeviceId).toBe('default')
    expect(store.outputDeviceName).toBe('System Default')
    expect(readPersistedSettings(storage).outputDeviceId).toBe('default')
    expect(readPersistedSettings(storage).outputDeviceName).toBe('System Default')
    expect(store.currentOutputDevice?.id).toBe('default')
  })

  it('keeps user preferences intact while strict BitPerfect is enabled', async () => {
    const store = useConfigStore()

    store.softwareVolume = true
    store.exclusiveMode = false
    expect(store.softwareVolume).toBe(true)

    // 开启严格 BitPerfect：软件音量在派生层失效（canAdjustVolume=false），
    // 但不得把用户的 softwareVolume / exclusiveMode 偏好覆写成开关含义——
    // 否则关掉 BitPerfect 后偏好已被永久销毁，用户要重新手动打开。
    store.strictBitPerfect = true
    expect(store.strictBitPerfect).toBe(true)
    expect(store.softwareVolume).toBe(true)
    expect(store.exclusiveMode).toBe(false)
    expect(store.canAdjustVolume).toBe(false)
    // BitPerfect 隐含独占在每次下发时派生，无需锁存 exclusiveMode。
    expect(store.outputOptionsFor('hw:CARD=1,DEV=0')).toEqual({
      strictBitPerfect: true,
      exclusive: true
    })

    store.softwareVolume = true
    expect(store.softwareVolume).toBe(true)

    await flushPersistedSettings()
    expect(readPersistedSettings(storage)).toMatchObject({
      strictBitPerfect: true,
      exclusiveMode: false,
      softwareVolume: true
    })

    // 关掉 BitPerfect：偏好原样恢复，无需任何“恢复”逻辑。
    store.strictBitPerfect = false
    expect(store.softwareVolume).toBe(true)
    expect(store.canAdjustVolume).toBe(true)
  })

  it('keeps output preferences when switching to the WebAPI engine', async () => {
    const store = useConfigStore()

    store.softwareVolume = true
    store.strictBitPerfect = true
    store.audioEngine = 'webapi'
    // 偏好保留；WebAPI 引擎不经过原生输出链路，独占/BitPerfect 只是不生效。
    expect(store.strictBitPerfect).toBe(true)
    expect(store.outputOptionsFor('hw:CARD=1,DEV=0')).toEqual({
      strictBitPerfect: false,
      exclusive: false
    })
    expect(store.supportsExclusiveOutput).toBe(false)
    // WebAPI 没有 BitPerfect 概念，软件音量照常可调。
    expect(store.canAdjustVolume).toBe(true)

    await flushPersistedSettings()
    expect(readPersistedSettings(storage)).toMatchObject({
      audioEngine: 'webapi',
      strictBitPerfect: true
    })

    // 切回 Native 后偏好原样恢复。
    store.audioEngine = 'native'
    expect(store.outputOptionsFor('hw:CARD=1,DEV=0')).toEqual({
      strictBitPerfect: true,
      exclusive: true
    })
  })

  it('restores raw output preferences persisted alongside the WebAPI engine', () => {
    writePersistedSettings(storage, {
      audioEngine: 'webapi',
      strictBitPerfect: true,
      exclusiveMode: true,
      softwareVolume: true
    })

    const store = useConfigStore()

    // 加载期不再归一化掉偏好：原样读入，生效与否由派生层决定。
    expect(store.audioEngine).toBe('webapi')
    expect(store.strictBitPerfect).toBe(true)
    expect(store.exclusiveMode).toBe(true)
    expect(store.softwareVolume).toBe(true)
    expect(store.outputOptionsFor('hw:CARD=1,DEV=0')).toEqual({
      strictBitPerfect: false,
      exclusive: false
    })
  })

  it('defaults application volume to 100 and clamps persisted values into range', () => {
    expect(useConfigStore().volumePercent).toBe(100)

    setActivePinia(createPinia())
    writePersistedSettings(storage, { volumePercent: 350 })
    expect(useConfigStore().volumePercent).toBe(100)

    setActivePinia(createPinia())
    writePersistedSettings(storage, { volumePercent: -20 })
    expect(useConfigStore().volumePercent).toBe(0)

    setActivePinia(createPinia())
    writePersistedSettings(storage, { volumePercent: 42.6 })
    expect(useConfigStore().volumePercent).toBe(43)
  })

  it('persists application volume changes', async () => {
    const store = useConfigStore()

    store.volumePercent = 55
    await flushPersistedSettings()

    expect(readPersistedSettings(storage).volumePercent).toBe(55)
  })

  it('defaults fullscreen favorite/play-mode buttons to visible and persists toggles', async () => {
    const store = useConfigStore()

    expect(store.fullscreenShowFavorite).toBe(true)
    expect(store.fullscreenShowPlayMode).toBe(true)

    store.fullscreenShowFavorite = false
    store.fullscreenShowPlayMode = false
    await flushPersistedSettings()

    expect(readPersistedSettings(storage)).toMatchObject({
      fullscreenShowFavorite: false,
      fullscreenShowPlayMode: false
    })

    setActivePinia(createPinia())
    const reloaded = useConfigStore()
    expect(reloaded.fullscreenShowFavorite).toBe(false)
    expect(reloaded.fullscreenShowPlayMode).toBe(false)
  })

  it('defaults lyric debug to disabled and persists explicit changes', async () => {
    const store = useConfigStore()

    expect(store.lyricDebug).toBe(false)
    expect(readPersistedSettings(storage).lyricDebug).toBeUndefined()

    store.lyricDebug = true
    await flushPersistedSettings()

    expect(readPersistedSettings(storage).lyricDebug).toBe(true)

    store.lyricDebug = false
    await flushPersistedSettings()

    expect(readPersistedSettings(storage).lyricDebug).toBe(false)
  })

  it('loads only boolean lyric debug values from persisted settings', () => {
    writePersistedSettings(storage, {
      lyricDebug: 'true'
    })

    expect(useConfigStore().lyricDebug).toBe(false)
  })

  it('restores lyric debug when persisted settings enable it', () => {
    writePersistedSettings(storage, {
      lyricDebug: true
    })

    expect(useConfigStore().lyricDebug).toBe(true)
  })

  it('applies sound quality and debug changes written by another window', async () => {
    let storageListener: ((event: StorageEvent) => void) | undefined

    vi.stubGlobal('window', {
      api,
      addEventListener: vi.fn((eventName: string, listener: EventListener) => {
        if (eventName === 'storage') {
          storageListener = listener as (event: StorageEvent) => void
        }
      })
    } as unknown as Window & typeof globalThis)

    writePersistedSettings(storage, {
      soundQuality: 'standard',
      lyricDebug: false
    })

    const store = useConfigStore()
    expect(store.soundQuality).toBe('standard')
    expect(store.lyricDebug).toBe(false)

    writePersistedSettings(storage, {
      soundQuality: 'jymaster',
      lyricDebug: true
    })

    storageListener?.({ key: STORAGE_KEY } as StorageEvent)
    await flushPersistedSettings()

    expect(store.soundQuality).toBe('jymaster')
    expect(store.lyricDebug).toBe(true)
  })

  /**
   * 回归：开着独占/BitPerfect 时也要能切回“系统默认输出”。
   *
   * 系统默认输出是混音服务器的虚拟端点，原生侧会以“不是真实硬件设备”拒绝独占与
   * BitPerfect。若照着用户存的偏好原样下发，这条切换必然失败并回滚——用户被锁在
   * 自己的 DAC 上，连把开关关掉都没用（那只影响下一首，当前会话的选项是开播时
   * 冻结的）。选项必须按“切过去之后”的端点算。
   */
  it('drops exclusive and bit-perfect when switching to the system default output', async () => {
    const preferredDeviceId = 'hw:CARD=1,DEV=0'
    writePersistedSettings(storage, {
      outputDeviceId: preferredDeviceId,
      outputDeviceName: 'FreeDSP Mini',
      exclusiveMode: true,
      strictBitPerfect: true
    })

    api.get_output_devices.mockResolvedValue([
      createDevice(preferredDeviceId, { name: 'FreeDSP Mini', isCurrent: true })
    ])
    api.switch_output_device.mockResolvedValue(undefined)

    const store = useConfigStore()
    expect(store.strictBitPerfect).toBe(true)
    expect(store.supportsExclusiveOutput).toBe(true)

    expect(await store.setOutputDevice('default')).toBe(true)

    expect(api.switch_output_device).toHaveBeenLastCalledWith(undefined, false, false)
    // 用户存的偏好保持原样，重新选回硬件端点即自动恢复。
    expect(store.strictBitPerfect).toBe(true)
    expect(store.exclusiveMode).toBe(true)
    expect(store.supportsExclusiveOutput).toBe(false)
    expect(store.outputOptionsFor('default')).toEqual({
      strictBitPerfect: false,
      exclusive: false
    })
    expect(store.outputOptionsFor(preferredDeviceId)).toEqual({
      strictBitPerfect: true,
      exclusive: true
    })

    api.get_output_devices.mockResolvedValue([
      createDevice(preferredDeviceId, { name: 'FreeDSP Mini', isCurrent: true })
    ])
    expect(await store.setOutputDevice(preferredDeviceId)).toBe(true)
    expect(api.switch_output_device).toHaveBeenLastCalledWith(preferredDeviceId, true, true)
  })

  // ====== 设备 id 写法分裂（热插拔后“设备在却显示不可用”的根因）======

  /**
   * 回归：同一端点的持久化写法（数字卡号）与列表条目写法（字符卡名）不同时，
   * 激活判定必须经别名表命中——裸字符串比较会恒判“未激活”，每次开播都重复
   * 一次注定 no-op 的切换 IPC，设置页还把设备显示成“当前不可用”。
   */
  it('treats the configured device as active when the listed entry uses a different id spelling', async () => {
    const preferredDeviceId = 'hw:CARD=1,DEV=0'
    writePersistedSettings(storage, {
      outputDeviceId: preferredDeviceId,
      outputDeviceName: 'USB DAC'
    })

    api.get_output_devices.mockResolvedValue([
      createDevice('hw:CARD=UsbDac,DEV=0', {
        name: 'USB DAC',
        isCurrent: true,
        aliases: ['hw:CARD=UsbDac,DEV=0', 'hw:CARD=1,DEV=0']
      })
    ])

    const store = useConfigStore()
    const appliedDeviceId = await store.ensureConfiguredOutputDevice()

    expect(appliedDeviceId).toBe('hw:CARD=UsbDac,DEV=0')
    // 写法迁移：持久化身份对齐到条目的规范写法（字符卡名跨拔插稳定，
    // 数字卡号在 USB 拔插后可能漂移），存储随之更新（经 120ms 去抖）。
    expect(store.outputDeviceId).toBe('hw:CARD=UsbDac,DEV=0')
    await flushPersistedSettings()
    expect(readPersistedSettings(storage).outputDeviceId).toBe('hw:CARD=UsbDac,DEV=0')
    // 不再发起注定 no-op 的切换。
    expect(api.switch_output_device).not.toHaveBeenCalled()
    // 列表不再拼出多余的“当前不可用”条目。
    expect(store.outputDevices.map((device) => device.id)).toEqual(['hw:CARD=UsbDac,DEV=0'])
  })

  /**
   * 回归：插回后卡号漂移（1→2）导致持久化的数字写法真的找不到时，仍然回退
   * 系统默认，但设备偏好保持原样（不因运行时回退被改写）。
   */
  it('keeps the persisted numeric spelling when the endpoint truly disappears', async () => {
    const preferredDeviceId = 'hw:CARD=1,DEV=0'
    writePersistedSettings(storage, {
      outputDeviceId: preferredDeviceId,
      outputDeviceName: 'USB DAC'
    })

    api.get_output_devices.mockResolvedValue([
      createDevice('hw:CARD=2,DEV=0', {
        name: 'USB DAC',
        isDefault: true,
        isCurrent: true
      })
    ])
    api.switch_output_device.mockImplementation(async (deviceId?: string) => {
      if (deviceId === preferredDeviceId) {
        throw new Error('device not found')
      }
    })

    const store = useConfigStore()
    const appliedDeviceId = await store.ensureConfiguredOutputDevice()

    expect(appliedDeviceId).toBe('default')
    expect(store.outputDeviceId).toBe(preferredDeviceId)
    expect(readPersistedSettings(storage).outputDeviceId).toBe(preferredDeviceId)
  })

  /**
   * 回归：别名命中时刷新列表要把持久化写法迁移到规范写法——这是“卡号漂移后
   * 旧 id 永久失效”的自愈路径（同一 DAC 插回，字符卡名写法不变即可匹配）。
   */
  it('migrates the stored device id spelling to the canonical entry on refresh', async () => {
    writePersistedSettings(storage, {
      outputDeviceId: 'hw:CARD=1,DEV=0',
      outputDeviceName: 'USB DAC'
    })

    api.get_output_devices.mockResolvedValue([
      createDevice('hw:CARD=UsbDac,DEV=0', {
        name: 'USB DAC',
        isCurrent: false,
        aliases: ['hw:CARD=UsbDac,DEV=0', 'hw:CARD=1,DEV=0']
      })
    ])

    const store = useConfigStore()
    await store.refreshOutputDevices()

    expect(store.outputDeviceId).toBe('hw:CARD=UsbDac,DEV=0')
    expect(store.outputDevices[0]?.aliases).toContain('hw:CARD=1,DEV=0')
  })
})
