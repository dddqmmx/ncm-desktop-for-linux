import { DEFAULT_SETTINGS, type ThemeMode } from '@renderer/stores/config/types'

function normalizeHexColor(color: string): string {
  return /^#[0-9a-fA-F]{6}$/.test(color) ? color.toLowerCase() : DEFAULT_SETTINGS.accentColor
}

function hexToRgb(color: string): [number, number, number] {
  const normalizedColor = normalizeHexColor(color)
  return [
    Number.parseInt(normalizedColor.slice(1, 3), 16),
    Number.parseInt(normalizedColor.slice(3, 5), 16),
    Number.parseInt(normalizedColor.slice(5, 7), 16)
  ]
}

function mixChannel(channel: number, target: number, weight: number): number {
  return Math.round(channel + (target - channel) * weight)
}

function mixColor(color: string, target: number, weight: number): string {
  const [red, green, blue] = hexToRgb(color)
  return `rgb(${mixChannel(red, target, weight)}, ${mixChannel(green, target, weight)}, ${mixChannel(
    blue,
    target,
    weight
  )})`
}

export function applyThemeColor(color: string): void {
  if (typeof document === 'undefined' || !document.documentElement) {
    return
  }

  const normalizedColor = normalizeHexColor(color)
  const [red, green, blue] = hexToRgb(normalizedColor)
  const rootStyle = document.documentElement.style

  rootStyle.setProperty('--theme-color', normalizedColor)
  rootStyle.setProperty('--theme-color-rgb', `${red}, ${green}, ${blue}`)
  rootStyle.setProperty('--theme-color-soft', `rgba(${red}, ${green}, ${blue}, 0.12)`)
  rootStyle.setProperty('--theme-color-muted', `rgba(${red}, ${green}, ${blue}, 0.2)`)
  rootStyle.setProperty('--theme-color-strong', mixColor(normalizedColor, 0, 0.18))
  rootStyle.setProperty('--theme-color-tint', mixColor(normalizedColor, 255, 0.82))
}

function resolveThemeMode(theme: ThemeMode): 'light' | 'dark' {
  if (theme !== 'adaptive') {
    return theme
  }

  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return 'light'
  }

  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

/**
 * 窗口底色（`BrowserWindow` 的 backgroundColor）与 `--sys-bg` 保持一致。
 *
 * 页面 html/body 是透明的——玻璃材质要靠透明才能让背景的模糊透上来——于是窗口
 * 底色会从页面边缘那条亚像素缝里露出一线。Electron 默认底色是纯白，深色主题下
 * 就是贴着窗口上沿的一条白边。把窗口底色对齐当前主题的底色，这条缝就看不见了。
 */
const WINDOW_BACKGROUND: Record<'light' | 'dark', string> = {
  light: '#eef1f6',
  dark: '#101217'
}

export function applySystemTheme(theme: ThemeMode, accentColor: string): void {
  if (typeof document === 'undefined' || !document.documentElement) {
    return
  }

  applyThemeColor(accentColor)

  const resolvedTheme = resolveThemeMode(theme)
  const root = document.documentElement
  root.dataset.theme = resolvedTheme
  root.style.colorScheme = resolvedTheme

  // 主窗口才有这条缝（设置/对话框窗口是圆角透明确实需要透明底），主进程按
  // 发送方过滤过，这里直接下发即可。preload 尚未更新时静默跳过。
  void window.api?.set_window_background?.(WINDOW_BACKGROUND[resolvedTheme])
}
