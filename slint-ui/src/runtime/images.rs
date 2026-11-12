use futures_util::{stream, StreamExt};
use serde_json::Value;
use slint::{Rgba8Pixel, SharedPixelBuffer};
use std::{
    collections::HashSet,
    io::{BufRead, Cursor, Seek},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{mpsc, Semaphore};

// Fullscreen requests 1600px for HiDPI. Home cards use physical display pixels;
// keep intermediate sizes so a 256px request doesn't become an 800px texture.
const COVER_SIZE: u32 = 800;
pub(super) fn cover_size(requested: u32) -> u32 {
    [128, 256, 384, 512, 800, 1600]
        .into_iter()
        .find(|&size| size >= requested)
        .unwrap_or(1600)
}
pub const QUEUE_CAPACITY: usize = 8;
pub const UPDATES_PER_TICK: usize = 4;

pub struct DecodedImage {
    pub target: ImageTarget,
    pub epoch: u64,
    pub pixels: SharedPixelBuffer<Rgba8Pixel>,
    pub palette: Option<crate::fullscreen_palette::CoverPalette>,
}

pub enum ImageTarget {
    Cover(String, u32),
    LoginQr(u64),
}

pub struct ImageLoader {
    cache: Arc<native::CacheService>,
    sender: mpsc::Sender<DecodedImage>,
    decoders: Arc<Semaphore>,
    downloads: Semaphore,
    requested: Mutex<CoverRequests>,
}

#[derive(Default)]
struct CoverRequests {
    epoch: u64,
    urls: HashSet<(String, u32)>,
}

// Deduplicate only in-flight work. Evicted UI images must be reloadable from disk.
struct CoverRequest<'a> {
    requested: &'a Mutex<CoverRequests>,
    key: (u64, String, u32),
}
impl Drop for CoverRequest<'_> {
    fn drop(&mut self) {
        let mut requested = self.requested.lock().unwrap();
        if requested.epoch == self.key.0 {
            requested.urls.remove(&(self.key.1.clone(), self.key.2));
        }
    }
}

impl ImageLoader {
    pub fn new(cache: Arc<native::CacheService>, sender: mpsc::Sender<DecodedImage>) -> Self {
        Self {
            cache,
            sender,
            decoders: Arc::new(Semaphore::new(2)),
            downloads: Semaphore::new(8),
            requested: Mutex::new(CoverRequests::default()),
        }
    }

    pub async fn load(&self, body: &Value, epoch: u64, current_epoch: &AtomicU64) {
        let mut urls = Vec::new();
        let mut seen = HashSet::new();
        for request in body.as_array().into_iter().flatten() {
            let size = cover_size(request["size"].as_u64().unwrap_or(800).min(1600) as u32);
            let Some(url) = request["cover"]
                .as_str()
                .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
            else {
                continue;
            };
            if seen.insert((url.to_owned(), size)) {
                urls.push((url.to_owned(), size));
            }
        }
        stream::iter(urls)
            .for_each_concurrent(8, |(url, size)| async move {
                if epoch != current_epoch.load(Ordering::Acquire) || self.sender.is_closed() {
                    return;
                }
                let key = (epoch, url.clone(), size);
                {
                    let mut requested = self.requested.lock().unwrap();
                    if epoch != current_epoch.load(Ordering::Acquire) {
                        return;
                    }
                    if requested.epoch != epoch {
                        requested.epoch = epoch;
                        requested.urls.clear();
                    }
                    if !requested.urls.insert((url.clone(), size)) {
                        return;
                    }
                }
                let _request = CoverRequest {
                    requested: &self.requested,
                    key,
                };
                let Ok(_download) = self.downloads.acquire().await else {
                    return;
                };
                if epoch != current_epoch.load(Ordering::Acquire) {
                    return;
                }
                let source = cover_url(&url, size);
                let result = tokio::time::timeout(
                    Duration::from_secs(8),
                    self.cache
                        .cache_remote_file("cover".into(), source.clone(), source),
                )
                .await;
                let Ok(Ok(Some(path))) = result else { return };
                // Reserve capacity before decoding: no producer may accumulate
                // decoded buffers outside the bounded queue while the UI is busy.
                let Ok(slot) = self.sender.reserve().await else {
                    return;
                };
                let Ok(permit) = self.decoders.clone().acquire_owned().await else {
                    return;
                };
                if epoch != current_epoch.load(Ordering::Acquire) || self.sender.is_closed() {
                    return;
                }
                // Disk reads, decoding, resizing and pixel allocation must all stay
                // off the Slint thread, including when the disk cache is warm.
                let decoded = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    let pixels = decode(Path::new(&path), size)?;
                    let palette = crate::fullscreen_palette::extract(&pixels);
                    Ok::<_, String>((pixels, palette))
                })
                .await;
                if let Ok(Ok((pixels, palette))) = decoded {
                    if epoch == current_epoch.load(Ordering::Acquire) {
                        slot.send(DecodedImage {
                            target: ImageTarget::Cover(url, size),
                            epoch,
                            pixels,
                            palette,
                        });
                    }
                }
            })
            .await;
    }

    pub async fn login_qr(&self, data: String, epoch: u64, token: u64) -> Result<(), String> {
        let slot = self.sender.reserve().await.map_err(|e| e.to_string())?;
        let permit = self
            .decoders
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let pixels = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            use base64::Engine;
            let encoded = data
                .strip_prefix("data:image/png;base64,")
                .ok_or("无效的登录二维码")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|e| e.to_string())?;
            decode_reader(
                image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png),
                COVER_SIZE,
            )
        })
        .await
        .map_err(|e| e.to_string())??;
        slot.send(DecodedImage {
            target: ImageTarget::LoginQr(token),
            epoch,
            pixels,
            palette: None,
        });
        Ok(())
    }
}

fn cover_url(source: &str, size: u32) -> String {
    let Ok(mut url) = reqwest::Url::parse(source) else {
        return source.to_owned();
    };
    // This is an optional CDN optimization; other API providers keep their URLs.
    if !url
        .host_str()
        .is_some_and(|host| host == "music.126.net" || host.ends_with(".music.126.net"))
    {
        return source.to_owned();
    }
    let pairs: Vec<_> = url
        .query_pairs()
        .filter(|(k, _)| k != "param")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    url.query_pairs_mut()
        .clear()
        .extend_pairs(pairs)
        .append_pair("param", &format!("{size}y{size}"));
    url.into()
}

fn decode(path: &Path, size: u32) -> Result<SharedPixelBuffer<Rgba8Pixel>, String> {
    let reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    decode_reader(reader, size)
}

fn decode_reader<R: BufRead + Seek>(
    mut reader: image::ImageReader<R>,
    size: u32,
) -> Result<SharedPixelBuffer<Rgba8Pixel>, String> {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|e| e.to_string())?;
    let pixels = if decoded.width() > size || decoded.height() > size {
        decoded.thumbnail(size, size).into_rgba8()
    } else {
        decoded.into_rgba8()
    };
    Ok(SharedPixelBuffer::clone_from_slice(
        pixels.as_raw(),
        pixels.width(),
        pixels.height(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdn_thumbnails_preserve_custom_provider_urls_and_other_parameters() {
        assert_eq!(
            cover_url("https://p1.music.126.net/a.jpg", 128),
            "https://p1.music.126.net/a.jpg?param=128y128"
        );
        assert_eq!(
            cover_url("https://p1.music.126.net/a.jpg?param=3000y3000&x=1", 800),
            "https://p1.music.126.net/a.jpg?x=1&param=800y800"
        );
        assert_eq!(
            cover_url("https://p1.music.126.net/a.jpg?param=128y128", 1600),
            "https://p1.music.126.net/a.jpg?param=1600y1600"
        );
        for url in [
            "https://other.example/a.jpg?token=abc",
            "https://music.126.net.example/a.jpg",
        ] {
            assert_eq!(cover_url(url, 128), url);
        }
    }

    #[test]
    fn originals_are_decoded_and_scaled_before_crossing_threads() {
        let path = super::super::tests::temp("cover").join("original.cache");
        image::RgbaImage::from_pixel(1600, 1000, image::Rgba([12, 34, 56, 128]))
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        let input = path.clone();
        let pixels = std::thread::spawn(move || decode(&input, 800).unwrap())
            .join()
            .unwrap();
        assert_eq!((pixels.width(), pixels.height()), (800, 500));
        assert_eq!(&pixels.as_bytes()[..4], &[12, 34, 56, 128]);
        let thumbnail = decode(&path, 128).unwrap();
        assert_eq!((thumbnail.width(), thumbnail.height()), (128, 80));
        let artwork = decode(&path, 1600).unwrap();
        assert_eq!((artwork.width(), artwork.height()), (1600, 1000));
        image::RgbaImage::new(9000, 1)
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        assert!(
            decode(&path, 800).is_err(),
            "oversized originals must respect decoding limits"
        );
        std::fs::write(&path, b"not an image").unwrap();
        assert!(decode(&path, 800).is_err());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn completed_cover_requests_can_be_loaded_again_after_ui_eviction() {
        let requested = Mutex::new(CoverRequests {
            epoch: 1,
            urls: HashSet::new(),
        });
        for _ in 0..2 {
            assert!(requested.lock().unwrap().urls.insert(("cover".into(), 128)));
            let _request = CoverRequest {
                requested: &requested,
                key: (1, "cover".into(), 128),
            };
        }
        assert!(requested.lock().unwrap().urls.is_empty());
    }

    #[tokio::test]
    async fn login_qr_uses_the_bounded_pixel_channel_with_request_identity() {
        use base64::Engine;
        let path = super::super::tests::temp("qr");
        let cache =
            Arc::new(native::CacheService::new(path.to_string_lossy().into_owned(), None).unwrap());
        let (tx, mut rx) = mpsc::channel(1);
        let loader = ImageLoader::new(cache, tx);
        let mut encoded = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 3, image::Rgba([0, 0, 0, 255]))
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let data = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(encoded.into_inner())
        );
        loader.login_qr(data.clone(), 7, 11).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(10), loader.login_qr(data, 8, 12))
                .await
                .is_err(),
            "full pixel queue must apply backpressure"
        );
        let image = rx.recv().await.unwrap();
        assert!(matches!(image.target, ImageTarget::LoginQr(11)));
        assert_eq!(image.epoch, 7);
        assert_eq!((image.pixels.width(), image.pixels.height()), (2, 3));
        assert!(loader.login_qr("invalid".into(), 7, 11).await.is_err());
        assert!(rx.try_recv().is_err());
        drop(loader);
        std::fs::remove_dir_all(path).unwrap();
    }
}
