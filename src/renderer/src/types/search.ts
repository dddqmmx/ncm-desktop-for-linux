
interface Artist {
  id: number;
  name: string;
  img1v1Url: string; // 对应 JSON 中的头像链接
  picId: number;     // JSON 中是 number 类型
  alias: string[];   // 别名数组
  albumSize?: number;
  musicSize?: number;
  img1v1?: number;
  trans?: string;    // 有时会有翻译名
}

/**
 * 专辑信息
 */
interface Album {
  id: number;
  name: string;
  picId: number;     // JSON 中是大整数，但类型为 number
  publishTime: number; // 发布时间戳
  size: number;      // 包含歌曲数量
  copyrightId: number;
  status: number;
  mark: number;
  artist: Artist;    // 专辑通常归属一位主要歌手
  transNames?: string[]; // 专辑译名，例如 ["Leo/need Another Vocal Album"]
}

/**
 * 歌曲详情
 */
interface Song {
  id: number;
  name: string;
  duration: number;  // 时长 (毫秒)
  artists: Artist[]; // 一首歌可能有多个歌手
  album: Album;

  // 以下是 JSON 中补充的字段
  fee: number;       // 费用/版权标识 (0: 免费, 1: VIP, 8: 免费但可能有版权限制等)
  status: number;    // 歌曲状态
  mark: number;      // 标记位
  copyrightId: number;
  alias: string[];   // 歌曲别名
  mvid: number;      // MV 的 ID，0 表示无 MV
  rtype: number;
  ftype: number;
}

/**
 * 搜索结果主体 (对应 result)
 */
interface SearchResult {
  songs: Song[];
  songCount: number;
  hasMore: boolean;
}
