
import { describe, it, expect, vi } from 'vitest';

vi.mock('electron', () => ({
  app: {
    getPath: vi.fn().mockReturnValue('/tmp'),
  },
  protocol: {
    registerSchemesAsPrivileged: vi.fn(),
  },
}));

vi.mock('NeteaseCloudMusicApi', () => ({
  login_cellphone: vi.fn(),
  user_cloud: vi.fn(),
  banner: vi.fn(),
  song_detail: vi.fn(),
  login_qr_key: vi.fn(),
  login_qr_create: vi.fn(),
  login_qr_check: vi.fn(),
  search: vi.fn(),
  cloudsearch: vi.fn(),
  user_account: vi.fn(),
  song_url_v1: vi.fn(),
  playlist_catlist: vi.fn(),
  user_playlist: vi.fn(),
  playlist_detail: vi.fn(),
  lyric_new: vi.fn(),
  recommend_resource: vi.fn(),
  recommend_songs: vi.fn(),
  artist_detail: vi.fn(),
  artist_top_song: vi.fn(),
  artist_album: vi.fn(),
  artist_mv: vi.fn(),
  album: vi.fn(),
}));

// Mock loadNativeModule to avoid native binding errors in tests
vi.mock('../native/loadNativeModule', () => ({
  getNativeModule: vi.fn().mockReturnValue(null),
}));

import { MusicService } from '../musicService';
import * as Netease from 'NeteaseCloudMusicApi';

describe('MusicService search mapping', () => {
  it('should map cloudsearch result fields to legacy search format', async () => {
    const mockSongs = [
      {
        id: 1,
        name: 'Song 1',
        ar: [{ id: 10, name: 'Artist 1' }],
        al: { id: 100, name: 'Album 1' },
        dt: 180000,
      }
    ];

    (Netease.cloudsearch as any).mockResolvedValue({
      status: 200,
      body: {
        result: {
          songs: mockSongs,
          songCount: 1
        }
      }
    });

    const res = await MusicService.search({ keywords: 'test' });
    const song = (res.body as any).result.songs[0];

    expect(song.artists).toEqual(mockSongs[0].ar);
    expect(song.album).toEqual(mockSongs[0].al);
    expect(song.duration).toBe(mockSongs[0].dt);
    expect(song.name).toBe('Song 1');
  });
});
