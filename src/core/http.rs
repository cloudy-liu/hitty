use anyhow::{Context as _, Result};
use gpui::App;
use reqwest_client::ReqwestClient;

pub fn build_http_client(manual_proxy: Option<&str>) -> Result<ReqwestClient> {
    let user_agent = concat!("tty7/", env!("CARGO_PKG_VERSION"));
    if let Some(proxy) = manual_proxy
        .and_then(tty7_core::daemon::install::proxy::normalize_manual)
        .and_then(|url| gpui::http_client::Url::parse(&url).ok())
    {
        ReqwestClient::proxy_and_user_agent(Some(proxy), user_agent).context("building HTTP client")
    } else {
        ReqwestClient::user_agent(user_agent).context("building HTTP client")
    }
}

/// Configure GPUI's image loader as well as application HTTP requests.
pub fn init(cx: &mut App, manual_proxy: Option<&str>) -> Result<()> {
    cx.set_http_client(std::sync::Arc::new(build_http_client(manual_proxy)?));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Asset as _, ImageAssetLoader, Resource, TestAppContext};
    use smol::future::FutureExt as _;
    use std::io::{BufRead as _, Write as _};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    #[gpui::test]
    fn gui_http_loads_remote_images(cx: &mut TestAppContext) {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><rect width="24" height="12" fill="red"/></svg>"#;
        let image = load_from_http(cx, svg);
        assert_eq!(image.frame_count(), 1);
        // GPUI rasterizes SVGs at 2x for smoothing.
        assert_eq!(
            image.size(0),
            gpui::size(gpui::DevicePixels(48), gpui::DevicePixels(24))
        );

        let mut gif = Vec::new();
        image::codecs::gif::GifEncoder::new(&mut gif)
            .encode_frames(
                [[255, 0, 0, 255], [0, 255, 0, 255]]
                    .into_iter()
                    .map(|color| {
                        image::Frame::from_parts(
                            image::RgbaImage::from_pixel(24, 12, image::Rgba(color)),
                            0,
                            0,
                            image::Delay::from_numer_denom_ms(100, 1),
                        )
                    }),
            )
            .unwrap();
        assert_eq!(load_from_http(cx, &gif).frame_count(), 2);
    }

    fn load_from_http(cx: &mut TestAppContext, body: &[u8]) -> Arc<gpui::RenderImage> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/badge.svg", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let body = body.to_vec();
        let server = std::thread::spawn(move || {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if done.load(Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = std::io::BufReader::new(&stream);
            let mut line = String::new();
            loop {
                line.clear();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        });
        let result = cx.update(|cx| {
            init(cx, None).unwrap();
            smol::block_on(
                ImageAssetLoader::load(Resource::Uri(url.into()), cx).or(async {
                    smol::Timer::after(Duration::from_secs(5)).await;
                    Err(gpui::ImageCacheError::Other(Arc::new(anyhow::anyhow!(
                        "image request timed out"
                    ))))
                }),
            )
        });
        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();
        result.expect("the startup client must fetch and decode remote images")
    }

    #[gpui::test]
    fn gui_http_uses_configured_proxy(cx: &mut TestAppContext) {
        cx.update(|cx| {
            init(cx, Some("127.0.0.1:7890")).unwrap();
            assert_eq!(
                cx.http_client().proxy().unwrap().as_str(),
                "http://127.0.0.1:7890/"
            );
        });
    }
}
