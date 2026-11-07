//! Author photo loading boundary for History; local initials are rendered by the row.
use gpui::{App, AppContext as _, Context, Entity, RenderImage, Task};
use smol::io::AsyncReadExt as _;
use std::{
    collections::HashMap,
    io::{Cursor, Read as _, Write as _},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use tty7_core::daemon::install::checksums::{hex, sha256};

/// Use the preset's auto-sized initials box; pixel avatar sizes in the pinned
/// component force a half-size text box and inherit an unrelated line height.
pub(crate) fn author_avatar(
    name: impl Into<gpui::SharedString>,
    image: Option<Arc<RenderImage>>,
    size: f32,
) -> gpui_component::avatar::Avatar {
    use gpui::{Styled as _, prelude::FluentBuilder as _, px};
    use gpui_component::{Sizable as _, Size};
    gpui_component::avatar::Avatar::new()
        .name(name)
        .with_size(Size::Medium)
        .size(px(size))
        .line_height(gpui::relative(1.))
        .when_some(image, |avatar, image| {
            avatar
                .with_size(px(size))
                .src(gpui::ImageSource::Render(image))
        })
}

const PIXELS: u32 = 96;
const MAX_RESPONSE: u64 = 256 * 1024;
const MAX_ENTRIES: usize = 128;
const MAX_INFLIGHT: usize = 4;
const MAX_DISK_FILES: usize = 512;
const MAX_DISK_BYTES: u64 = 20 * 1024 * 1024;
const FRESH: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const RETRY: Duration = Duration::from_secs(2 * 60);

struct Entry {
    image: Option<Arc<RenderImage>>,
    retry_at: Instant,
    touched: Instant,
}

pub(crate) struct AvatarCache {
    directory: Option<PathBuf>,
    entries: HashMap<String, Entry>,
    jobs: HashMap<String, Task<()>>,
}

impl AvatarCache {
    pub(crate) fn new(directory: Option<PathBuf>, cx: &mut App) -> Entity<Self> {
        let cache = cx.new(|_| Self {
            directory,
            entries: HashMap::new(),
            jobs: HashMap::new(),
        });
        cx.observe_release(&cache, |cache, cx| {
            for entry in cache.entries.values_mut() {
                if let Some(image) = entry.image.take() {
                    discard_image(image, cx);
                }
            }
        })
        .detach();
        cache
    }

    pub(crate) fn image(
        &mut self,
        email: &str,
        cx: &mut Context<Self>,
    ) -> Option<Arc<RenderImage>> {
        let (key, url) = identity(email)?;
        let now = Instant::now();
        let image = self.entries.get_mut(&key).and_then(|entry| {
            entry.touched = now;
            entry.image.clone()
        });
        if self.jobs.contains_key(&key)
            || self
                .entries
                .get(&key)
                .is_some_and(|entry| now < entry.retry_at)
            || self.jobs.len() >= MAX_INFLIGHT
        {
            return image;
        }
        if !self.entries.contains_key(&key) && self.entries.len() >= MAX_ENTRIES {
            let victim = self
                .entries
                .iter()
                .filter(|(key, _)| !self.jobs.contains_key(*key))
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(key, _)| key.clone());
            let Some(victim) = victim else { return image };
            if let Some(entry) = self.entries.remove(&victim) {
                if let Some(image) = entry.image {
                    discard_image(image, cx);
                }
            }
        }
        self.entries.entry(key.clone()).or_insert(Entry {
            image: None,
            retry_at: now,
            touched: now,
        });
        let directory = self.directory.clone();
        let source_key = key.clone();
        let client = cx.http_client();
        let task = cx.spawn(async move |this, cx| {
            let disk_dir = directory.clone();
            let disk_key = source_key.clone();
            let cached = cx
                .background_executor()
                .spawn(async move { read_cached(disk_dir.as_deref(), &disk_key) })
                .await;
            if let Some((cached, remaining)) = cached {
                let fresh = !remaining.is_zero();
                if this
                    .update(cx, |cache, cx| {
                        if let Some(entry) = cache.entries.get_mut(&source_key) {
                            if let Some(old) = entry.image.replace(cached) {
                                discard_image(old, cx);
                            }
                            if fresh {
                                entry.retry_at = Instant::now() + remaining;
                            }
                        }
                        if fresh {
                            cache.jobs.remove(&source_key);
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
                if fresh {
                    return;
                }
            }
            let write_key = source_key.clone();
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let decoded = fetch(client, &url).await?;
                    if let Some(directory) = directory {
                        // Cache write failure must not hide an otherwise valid photo.
                        let _ = persist(&directory, &write_key, &decoded.png);
                    }
                    anyhow::Ok(decoded.image)
                })
                .await;
            let _ = this.update(cx, |cache, cx| {
                if let Some(entry) = cache.entries.get_mut(&source_key) {
                    match loaded {
                        Ok(image) => {
                            if let Some(old) = entry.image.replace(image) {
                                discard_image(old, cx);
                            }
                            entry.retry_at = Instant::now() + FRESH;
                        }
                        Err(_) => entry.retry_at = Instant::now() + RETRY,
                    }
                }
                cache.jobs.remove(&source_key);
                cx.notify();
            });
        });
        self.jobs.insert(key, task);
        image
    }
}

fn discard_image(image: Arc<RenderImage>, cx: &mut App) {
    // A cache lookup can evict during render, when the current window is
    // temporarily outside App.windows. Wait until that window is returned.
    cx.defer(move |cx| cx.drop_image(image, None));
}

fn identity(email: &str) -> Option<(String, String)> {
    let normalized = email.trim().to_lowercase();
    if normalized.len() > 320
        || normalized
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return None;
    }
    let (local, domain) = normalized.split_once('@')?;
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return None;
    }
    let key = hex(&sha256(normalized.as_bytes()));
    let url = if domain == "users.noreply.github.com" {
        if let Some((id, _)) = local.split_once('+').filter(|(id, _)| {
            !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())
        }) {
            format!("https://avatars.githubusercontent.com/u/{id}?s={PIXELS}&v=4")
        } else if local.len() <= 39
            && local
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            format!("https://github.com/{local}.png?size={PIXELS}")
        } else {
            format!("https://gravatar.com/avatar/{key}?s={PIXELS}&d=404")
        }
    } else {
        format!("https://gravatar.com/avatar/{key}?s={PIXELS}&d=404")
    };
    Some((key, url))
}

struct Decoded {
    image: Arc<RenderImage>,
    png: Vec<u8>,
}

fn decode(bytes: &[u8]) -> anyhow::Result<Decoded> {
    let (width, height) = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .into_dimensions()?;
    anyhow::ensure!(
        width > 0 && height > 0 && width <= 512 && height <= 512,
        "avatar dimensions exceed 512px"
    );
    let mut pixels = image::load_from_memory(bytes)?
        .thumbnail(PIXELS, PIXELS)
        .into_rgba8();
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(pixels.clone()).write_to(&mut png, image::ImageFormat::Png)?;
    // GPUI's native image atlas consumes straight-alpha BGRA.
    for pixel in pixels.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Ok(Decoded {
        image: Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(
            pixels
        )])),
        png: png.into_inner(),
    })
}

async fn fetch(
    client: Arc<dyn gpui::http_client::HttpClient>,
    url: &str,
) -> anyhow::Result<Decoded> {
    smol::future::or(
        async {
            let mut response = client.get(url, ().into(), true).await?;
            anyhow::ensure!(response.status().is_success(), "avatar HTTP error");
            let mut bytes = Vec::new();
            response
                .body_mut()
                .take(MAX_RESPONSE + 1)
                .read_to_end(&mut bytes)
                .await?;
            anyhow::ensure!(
                bytes.len() as u64 <= MAX_RESPONSE,
                "avatar response exceeds 256 KiB"
            );
            decode(&bytes)
        },
        async {
            smol::Timer::after(Duration::from_secs(8)).await;
            anyhow::bail!("avatar request timed out")
        },
    )
    .await
}

fn read_cached(directory: Option<&Path>, key: &str) -> Option<(Arc<RenderImage>, Duration)> {
    let file = std::fs::File::open(directory?.join(format!("{key}.png"))).ok()?;
    let metadata = file.metadata().ok()?;
    if metadata.len() > MAX_RESPONSE {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_RESPONSE + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_RESPONSE {
        return None;
    }
    let decoded = decode(&bytes).ok()?;
    let remaining = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .map(|age| FRESH.saturating_sub(age))
        .unwrap_or_default();
    Some((decoded.image, remaining))
}

fn persist(directory: &Path, key: &str, png: &[u8]) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(png)?;
    temporary.persist(directory.join(format!("{key}.png")))?;
    // Only files with our hashed names are eligible for eviction.
    let mut files: Vec<_> = std::fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let hash = name.strip_suffix(".png")?;
            if hash.len() != 64
                || !hash.bytes().all(|b| b.is_ascii_hexdigit())
                || !entry.file_type().ok()?.is_file()
            {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            Some((
                metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                metadata.len(),
                entry.path(),
            ))
        })
        .collect();
    let mut total: u64 = files.iter().map(|(_, bytes, _)| bytes).sum();
    let mut count = files.len();
    files.sort_by_key(|(modified, _, _)| *modified);
    for (_, bytes, path) in files {
        if count <= MAX_DISK_FILES && total <= MAX_DISK_BYTES {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total = total.saturating_sub(bytes);
            count -= 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        TestAppContext,
        http_client::{FakeHttpClient, Response},
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn photo() -> Vec<u8> {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            180,
            120,
            image::Rgba([100, 150, 200, 255]),
        ));
        let mut output = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut output, image::ImageFormat::Png)
            .unwrap();
        output.into_inner()
    }

    #[gpui::test]
    fn author_photo_is_shared_and_survives_an_offline_restart(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let bytes = photo();
        cx.update(|cx| {
            cx.set_http_client(FakeHttpClient::create(move |request| {
                assert_eq!(
                    request.uri().to_string(),
                    "https://avatars.githubusercontent.com/u/24760907?s=96&v=4"
                );
                let n = observed.fetch_add(1, Ordering::SeqCst);
                let bytes = bytes.clone();
                async move {
                    if n > 0 {
                        return Err(anyhow::anyhow!("offline"));
                    }
                    Ok(Response::builder().status(200).body(bytes.into()).unwrap())
                }
            }))
        });
        let directory = tempfile::tempdir().unwrap();
        let cache = cx.update(|cx| AvatarCache::new(Some(directory.path().to_path_buf()), cx));
        for _ in 0..20 {
            assert!(
                cache
                    .update(cx, |cache, cx| cache
                        .image("24760907+l0ng-ai@users.noreply.github.com", cx))
                    .is_none()
            );
        }
        cx.background_executor.run_until_parked();
        let first = cache
            .update(cx, |cache, cx| {
                cache.image("24760907+l0ng-ai@users.noreply.github.com", cx)
            })
            .expect("the author photo loads asynchronously");
        assert_eq!(
            first.size(0),
            gpui::size(gpui::DevicePixels(96), gpui::DevicePixels(64))
        );
        assert_eq!(
            &first.as_bytes(0).expect("decoded frame")[..4],
            &[200, 150, 100, 255],
            "the native photo preserves the source colors in GPUI's BGRA format"
        );
        let repeated = cache
            .update(cx, |cache, cx| {
                cache.image("24760907+L0NG-AI@USERS.NOREPLY.GITHUB.COM", cx)
            })
            .unwrap();
        assert!(
            Arc::ptr_eq(&first, &repeated),
            "repeated authors share decoded pixels"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        drop(cache);
        let restarted = cx.update(|cx| AvatarCache::new(Some(directory.path().to_path_buf()), cx));
        assert!(
            restarted
                .update(cx, |cache, cx| cache
                    .image("24760907+l0ng-ai@users.noreply.github.com", cx))
                .is_none()
        );
        cx.background_executor.run_until_parked();
        assert!(
            restarted
                .update(cx, |cache, cx| cache
                    .image("24760907+l0ng-ai@users.noreply.github.com", cx))
                .is_some(),
            "a new session must reuse the disk image without a network connection"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "warm cache makes no HTTP request"
        );
    }

    #[gpui::test]
    fn unavailable_or_unsafe_photos_keep_initials_without_request_loops(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let mut oversized = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(513, 1)
            .write_to(&mut oversized, image::ImageFormat::Png)
            .unwrap();
        let oversized = oversized.into_inner();
        cx.update(|cx| {
            cx.set_http_client(FakeHttpClient::create(move |request| {
                observed.fetch_add(1, Ordering::SeqCst);
                let uri = request.uri().to_string();
                let (status, bytes) = if uri.contains("/u/1?") {
                    (404, Vec::new())
                } else if uri.contains("/u/2?") {
                    (200, b"not an image".to_vec())
                } else if uri.contains("/u/3?") {
                    (200, vec![0; 256 * 1024 + 1])
                } else {
                    (200, oversized.clone())
                };
                async move {
                    Ok(Response::builder()
                        .status(status)
                        .body(bytes.into())
                        .unwrap())
                }
            }))
        });
        let cache = cx.update(|cx| AvatarCache::new(None, cx));
        for id in 1..=4 {
            cache.update(cx, |cache, cx| {
                assert!(
                    cache
                        .image(&format!("{id}+author@users.noreply.github.com"), cx)
                        .is_none()
                );
            });
        }
        cx.background_executor.run_until_parked();
        for _ in 0..100 {
            for id in 1..=4 {
                cache.update(cx, |cache, cx| {
                    assert!(
                        cache
                            .image(&format!("{id}+author@users.noreply.github.com"), cx)
                            .is_none()
                    );
                });
            }
        }
        for email in ["", "unknown", "a@", "a@b@c", "a b@example.com"] {
            assert!(
                cache
                    .update(cx, |cache, cx| cache.image(email, cx))
                    .is_none()
            );
        }
        cx.background_executor.run_until_parked();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            4,
            "failed photos are backed off, malformed identities never request"
        );
    }

    #[gpui::test]
    fn stale_disk_photo_remains_visible_when_refresh_is_offline(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let directory = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let bytes = photo();
        cx.update(|cx| {
            cx.set_http_client(FakeHttpClient::create(move |_| {
                let n = observed.fetch_add(1, Ordering::SeqCst);
                let bytes = bytes.clone();
                async move {
                    if n > 0 {
                        anyhow::bail!("offline");
                    }
                    Ok(Response::builder().status(200).body(bytes.into()).unwrap())
                }
            }))
        });
        let email = "ada@example.com";
        let cache = cx.update(|cx| AvatarCache::new(Some(directory.path().to_path_buf()), cx));
        cache.update(cx, |cache, cx| {
            cache.image(email, cx);
        });
        cx.background_executor.run_until_parked();
        assert!(
            cache
                .update(cx, |cache, cx| cache.image(email, cx))
                .is_some()
        );
        drop(cache);
        for file in std::fs::read_dir(directory.path()).unwrap() {
            std::fs::File::options()
                .write(true)
                .open(file.unwrap().path())
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(SystemTime::UNIX_EPOCH))
                .unwrap();
        }
        let restarted = cx.update(|cx| AvatarCache::new(Some(directory.path().to_path_buf()), cx));
        restarted.update(cx, |cache, cx| {
            cache.image(email, cx);
        });
        cx.background_executor.run_until_parked();
        for _ in 0..20 {
            assert!(
                restarted
                    .update(cx, |cache, cx| cache.image(email, cx))
                    .is_some(),
                "offline refresh retains the old photo"
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[gpui::test]
    fn image_work_is_bounded_and_repeated_history_has_no_warm_requests(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let calls = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let active_requests = active.clone();
        let peak_requests = peak.clone();
        let (release, gate) = smol::channel::unbounded::<()>();
        let bytes = photo();
        cx.update(|cx| {
            cx.set_http_client(FakeHttpClient::create(move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
                let n = active_requests.fetch_add(1, Ordering::SeqCst) + 1;
                peak_requests.fetch_max(n, Ordering::SeqCst);
                let gate = gate.clone();
                let active = active_requests.clone();
                let bytes = bytes.clone();
                async move {
                    gate.recv().await?;
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok(Response::builder().status(200).body(bytes.into()).unwrap())
                }
            }))
        });
        let cache = cx.update(|cx| AvatarCache::new(None, cx));
        for id in 1..=6 {
            cache.update(cx, |cache, cx| {
                cache.image(&format!("{id}+author@users.noreply.github.com"), cx);
            });
        }
        cx.background_executor.run_until_parked();
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        assert_eq!(active.load(Ordering::SeqCst), 4);
        for _ in 0..4 {
            release.try_send(()).unwrap();
        }
        cx.background_executor.run_until_parked();
        for id in 1..=6 {
            cache.update(cx, |cache, cx| {
                cache.image(&format!("{id}+author@users.noreply.github.com"), cx);
            });
        }
        cx.background_executor.run_until_parked();
        for _ in 0..2 {
            release.try_send(()).unwrap();
        }
        cx.background_executor.run_until_parked();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            6,
            "authors deferred at the limit can load on a later frame"
        );
        assert_eq!(peak.load(Ordering::SeqCst), 4);
        drop(cache);

        let directory = tempfile::tempdir().unwrap();
        let cold = cx.update(|cx| AvatarCache::new(Some(directory.path().to_path_buf()), cx));
        for _ in 0..3 {
            release.try_send(()).unwrap();
        }
        let before = calls.load(Ordering::SeqCst);
        for row in 0..5000 {
            cold.update(cx, |cache, cx| {
                cache.image(
                    &format!("{}+repeat@users.noreply.github.com", row % 3 + 10),
                    cx,
                );
            });
        }
        cx.background_executor.run_until_parked();
        let cold_requests = calls.load(Ordering::SeqCst) - before;
        assert_eq!(cold_requests, 3);
        for row in 0..5000 {
            assert!(
                cold.update(cx, |cache, cx| cache.image(
                    &format!("{}+repeat@users.noreply.github.com", row % 3 + 10),
                    cx
                ))
                .is_some()
            );
        }
        cx.background_executor.run_until_parked();
        assert_eq!(calls.load(Ordering::SeqCst) - before, 3);
        drop(cold);
        let warm = cx.update(|cx| AvatarCache::new(Some(directory.path().to_path_buf()), cx));
        for id in 10..13 {
            warm.update(cx, |cache, cx| {
                cache.image(&format!("{id}+repeat@users.noreply.github.com"), cx);
            });
        }
        cx.background_executor.run_until_parked();
        for id in 10..13 {
            assert!(
                warm.update(cx, |cache, cx| cache
                    .image(&format!("{id}+repeat@users.noreply.github.com"), cx))
                    .is_some()
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst) - before, 3);
        println!(
            "AVATAR_REQUEST_METRICS {{\"rows\":5000,\"authors\":3,\"cold_requests\":{cold_requests},\"memory_warm_requests\":0,\"disk_warm_requests\":0,\"peak_concurrency\":{}}}",
            peak.load(Ordering::SeqCst)
        );
    }
}
