import { CurrentSongArtist, CurrentSong, PersistedCurrentSong } from '@renderer/types/player'

export const UNKNOWN_ARTIST_NAME = '未知艺术家'

export const normalizeSingleCurrentSongArtist = (
  artist: { id?: number | null; name?: string | null } | string | null | undefined
): CurrentSongArtist | null => {
  if (typeof artist === 'string') {
    const name = artist.trim()
    return name
      ? {
          id: 0,
          name
        }
      : null
  }

  if (!artist || typeof artist !== 'object') return null

  const name = artist.name?.trim()
  if (!name) return null

  return {
    id: typeof artist.id === 'number' ? artist.id : 0,
    name
  }
}

export const createCurrentSongArtists = (
  artists: Array<{ id?: number | null; name?: string | null }> | null | undefined
): CurrentSongArtist[] => {
  const normalizedArtists = (Array.isArray(artists) ? artists : [])
    .map((artist) => normalizeSingleCurrentSongArtist(artist))
    .filter((artist): artist is CurrentSongArtist => artist !== null)

  return normalizedArtists.length > 0
    ? normalizedArtists
    : [
        {
          id: 0,
          name: UNKNOWN_ARTIST_NAME
        }
      ]
}

export const formatCurrentSongArtists = (
  artists: CurrentSongArtist[] | null | undefined
): string => {
  const names = (artists ?? []).map((artist) => artist.name).filter(Boolean)
  return names.length > 0 ? names.join(', ') : UNKNOWN_ARTIST_NAME
}

export const normalizeCurrentSongArtists = (
  artists: PersistedCurrentSong['artists'] | PersistedCurrentSong['artist']
): CurrentSongArtist[] => {
  if (Array.isArray(artists)) {
    const normalizedArtists = artists
      .map((artist) => normalizeSingleCurrentSongArtist(artist))
      .filter((artist): artist is CurrentSongArtist => artist !== null)

    return normalizedArtists.length > 0
      ? normalizedArtists
      : [
          {
            id: 0,
            name: UNKNOWN_ARTIST_NAME
          }
        ]
  }

  const normalizedArtist = normalizeSingleCurrentSongArtist(artists)
  return normalizedArtist
    ? [normalizedArtist]
    : [
        {
          id: 0,
          name: UNKNOWN_ARTIST_NAME
        }
      ]
}

export const normalizeCurrentSong = (song: PersistedCurrentSong): CurrentSong | null => {
  if (typeof song.id !== 'number' || !Number.isFinite(song.id)) return null

  return {
    id: song.id,
    name: typeof song.name === 'string' ? song.name : '未知歌曲',
    artists: normalizeCurrentSongArtists(song.artists ?? song.artist),
    cover: typeof song.cover === 'string' ? song.cover : '',
    duration:
      typeof song.duration === 'number' && Number.isFinite(song.duration) ? song.duration : 0
  }
}

export const loadPersistedPlaylist = (): CurrentSong[] => {
  try {
    const raw = JSON.parse(localStorage.getItem('playlist') || '[]') as unknown
    if (!Array.isArray(raw)) return []

    return raw
      .map((song) => normalizeCurrentSong(song as PersistedCurrentSong))
      .filter((song): song is CurrentSong => song !== null)
  } catch (error) {
    console.error('读取播放列表失败', error)
    return []
  }
}
