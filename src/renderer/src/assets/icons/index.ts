import artist_fallback from './artist-fallback.svg?raw'
import check from './check.svg?raw'
import chevron_down from './chevron-down.svg?raw'
import chevron_right from './chevron-right.svg?raw'
import clock from './clock.svg?raw'
import close from './close.svg?raw'
import close_fill from './close-fill.svg?raw'
import download from './download.svg?raw'
import equalizer from './equalizer.svg?raw'
import flask from './flask.svg?raw'
import folder from './folder.svg?raw'
import heart from './heart.svg?raw'
import heart_fill from './heart-fill.svg?raw'
import home from './home.svg?raw'
import image_placeholder from './image-placeholder.svg?raw'
import lock from './lock.svg?raw'
import loop from './loop.svg?raw'
import monitor from './monitor.svg?raw'
import more from './more.svg?raw'
import more_dots from './more-dots.svg?raw'
import more_fill from './more-fill.svg?raw'
import music from './music.svg?raw'
import music_fill from './music-fill.svg?raw'
import next from './next.svg?raw'
import noise from './noise.svg?raw'
import pause from './pause.svg?raw'
import phone from './phone.svg?raw'
import play from './play.svg?raw'
import play_alt from './play-alt.svg?raw'
import play_mv from './play-mv.svg?raw'
import playlist from './playlist.svg?raw'
import plus from './plus.svg?raw'
import prev from './prev.svg?raw'
import qr from './qr.svg?raw'
import search from './search.svg?raw'
import search_alt from './search-alt.svg?raw'
import search_line from './search-line.svg?raw'
import settings_about from './settings-about.svg?raw'
import settings_appearance from './settings-appearance.svg?raw'
import settings_audio from './settings-audio.svg?raw'
import settings_cache from './settings-cache.svg?raw'
import settings_debug from './settings-debug.svg?raw'
import settings_general from './settings-general.svg?raw'
import settings_library from './settings-library.svg?raw'
import settings_shortcuts from './settings-shortcuts.svg?raw'
import shuffle from './shuffle.svg?raw'
import single from './single.svg?raw'
import translate from './translate.svg?raw'
import trash from './trash.svg?raw'
import trash_outline from './trash-outline.svg?raw'

export const iconSvgMap = {
  'artist-fallback': artist_fallback,
  'check': check,
  'chevron-down': chevron_down,
  'chevron-right': chevron_right,
  'clock': clock,
  'close': close,
  'close-fill': close_fill,
  'download': download,
  'equalizer': equalizer,
  'flask': flask,
  'folder': folder,
  'heart': heart,
  'heart-fill': heart_fill,
  'home': home,
  'image-placeholder': image_placeholder,
  'lock': lock,
  'loop': loop,
  'monitor': monitor,
  'more': more,
  'more-dots': more_dots,
  'more-fill': more_fill,
  'music': music,
  'music-fill': music_fill,
  'next': next,
  'noise': noise,
  'pause': pause,
  'phone': phone,
  'play': play,
  'play-alt': play_alt,
  'play-mv': play_mv,
  'playlist': playlist,
  'plus': plus,
  'prev': prev,
  'qr': qr,
  'search': search,
  'search-alt': search_alt,
  'search-line': search_line,
  'settings-about': settings_about,
  'settings-appearance': settings_appearance,
  'settings-audio': settings_audio,
  'settings-cache': settings_cache,
  'settings-debug': settings_debug,
  'settings-general': settings_general,
  'settings-library': settings_library,
  'settings-shortcuts': settings_shortcuts,
  'shuffle': shuffle,
  'single': single,
  'translate': translate,
  'trash': trash,
  'trash-outline': trash_outline
} as const

export type IconName = keyof typeof iconSvgMap

export const iconNames = Object.keys(iconSvgMap) as IconName[]
