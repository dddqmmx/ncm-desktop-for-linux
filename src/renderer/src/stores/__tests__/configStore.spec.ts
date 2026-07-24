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

  beforeEach(() => {
    storage = new MemoryStorage()
    api = createApiMock()

    vi.stubGlobal('localStorage', storage)
    vi.stubGlobal('window', {
      api
    } as unknown as Window & typeof globalThis)

    vi.spyOn(console, 'error').mockImplementation(() => undefined)
    vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    setActivePinia(createPinia())
  })

  afterEach(() => {
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
    expect(api.switch_output_device.mock.calls).toEqual([[preferredDeviceId], [undefined]])
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
    await nextTick()

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
      [preferredDeviceId],
      [undefined],
      [preferredDeviceId]
    ])
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
    await nextTick()
    expect(store.outputDeviceId).toBe(preferredDeviceId)
    expect(store.outputDeviceName).toBe('USB DAC')
    expect(readPersistedSettings(storage).outputDeviceId).toBe(preferredDeviceId)
    expect(readPersistedSettings(storage).outputDeviceName).toBe('USB DAC')

    expect(await store.setOutputDevice('default')).toBe(true)
    await nextTick()
    expect(store.outputDeviceId).toBe('default')
    expect(store.outputDeviceName).toBe('System Default')
    expect(readPersistedSettings(storage).outputDeviceId).toBe('default')
    expect(readPersistedSettings(storage).outputDeviceName).toBe('System Default')
    expect(store.currentOutputDevice?.id).toBe('default')
  })

  it('prevents software volume while strict BitPerfect is enabled', async () => {
    const store = useConfigStore()

    store.softwareVolume = true
    expect(store.softwareVolume).toBe(true)

    store.strictBitPerfect = true
    expect(store.strictBitPerfect).toBe(true)
    expect(store.exclusiveMode).toBe(true)
    expect(store.softwareVolume).toBe(false)

    store.softwareVolume = true
    expect(store.softwareVolume).toBe(false)

    await nextTick()
    expect(readPersistedSettings(storage)).toMatchObject({
      strictBitPerfect: true,
      exclusiveMode: true,
      softwareVolume: false
    })
  })

  it('disables strict BitPerfect and exclusive output for the WebAPI engine', async () => {
    const store = useConfigStore()

    store.strictBitPerfect = true
    expect(store.exclusiveMode).toBe(true)

    store.audioEngine = 'webapi'
    expect(store.strictBitPerfect).toBe(false)
    expect(store.exclusiveMode).toBe(false)

    store.strictBitPerfect = true
    store.exclusiveMode = true
    expect(store.strictBitPerfect).toBe(false)
    expect(store.exclusiveMode).toBe(false)

    await nextTick()
    expect(readPersistedSettings(storage)).toMatchObject({
      audioEngine: 'webapi',
      strictBitPerfect: false,
      exclusiveMode: false
    })
  })

  it('normalizes incompatible persisted WebAPI output settings', () => {
    writePersistedSettings(storage, {
      audioEngine: 'webapi',
      strictBitPerfect: true,
      exclusiveMode: true,
      softwareVolume: true
    })

    const store = useConfigStore()

    expect(store.audioEngine).toBe('webapi')
    expect(store.strictBitPerfect).toBe(false)
    expect(store.exclusiveMode).toBe(false)
    expect(store.softwareVolume).toBe(true)
  })

  it('defaults lyric debug to disabled and persists explicit changes', async () => {
    const store = useConfigStore()

    expect(store.lyricDebug).toBe(false)
    expect(readPersistedSettings(storage).lyricDebug).toBeUndefined()

    store.lyricDebug = true
    await nextTick()

    expect(readPersistedSettings(storage).lyricDebug).toBe(true)

    store.lyricDebug = false
    await nextTick()

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
    await nextTick()

    expect(store.soundQuality).toBe('jymaster')
    expect(store.lyricDebug).toBe(true)
  })
})
