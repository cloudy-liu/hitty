//! Release downloads for the desktop updater.

use std::cell::Cell;
use std::ops::ControlFlow;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use gpui::http_client::{AsyncBody, HttpClient as _, HttpRequestExt as _, RedirectPolicy, Request};
use reqwest_client::ReqwestClient;
use smol::future::FutureExt as _;
use smol::io::AsyncReadExt as _;
use tty7_core::daemon::install::download::CANCELLED;
use tty7_core::daemon::install::proxy::{self, Proxy};

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const CANCEL_POLL: Duration = Duration::from_millis(100);
const MAX_CHECKSUM_BYTES: usize = 1024 * 1024;
const MAX_ASSET_BYTES: usize = 128 * 1024 * 1024;

async fn client(proxy: Option<&Proxy>) -> Result<ReqwestClient> {
    // Configure only this request's selected route. Automatic proxy discovery
    // and redirects must not override the caller's selection or bypass rule.
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .user_agent(concat!("tty7/", env!("CARGO_PKG_VERSION")))
        .use_preconfigured_tls(http_client_tls::tls_config());
    if let Some(proxy) = proxy {
        let mut proxy_url =
            reqwest::Url::parse(&proxy.uri().to_string()).context("parsing the update proxy")?;
        if proxy_url.scheme().starts_with("socks") {
            // Resolve only proxies the request actually needs, inside its
            // cancellation and timeout budgets. Numeric addresses keep
            // reqwest's synchronous SOCKS conversion from resolving again.
            let host = proxy_url
                .host_str()
                .context("the update proxy has no host")?;
            let address = smol::net::resolve((
                host.trim_matches(['[', ']']),
                proxy_url.port().unwrap_or(1080),
            ))
            .or(async {
                smol::Timer::after(CONNECT_TIMEOUT).await;
                Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "resolving the update proxy timed out",
                ))
            })
            .await
            .context("resolving the SOCKS update proxy")?
            .into_iter()
            .next()
            .context("the SOCKS update proxy resolved to no addresses")?;
            if proxy_url.set_ip_host(address.ip()).is_err() {
                bail!("the SOCKS update proxy has an invalid host");
            }
        }
        builder = builder.proxy(
            reqwest::Proxy::all(proxy_url.as_str()).context("configuring the update proxy")?,
        );
    }
    Ok(builder
        .build()
        .context("building the update download client")?
        .into())
}

pub(super) fn fetch(
    manual_proxy: Option<&str>,
    checksums_url: &str,
    asset_url: &str,
    on_progress: &dyn Fn(u64, Option<u64>) -> ControlFlow<()>,
) -> Result<(Vec<u8>, Vec<u8>)> {
    if on_progress(0, None).is_break() {
        bail!(CANCELLED);
    }
    let progress = Cell::new((0, None));
    smol::block_on(
        async {
            // Keep the existing manual > system > environment selection for
            // the attempt; each request below applies its bypass rules.
            let proxy = proxy::resolve("https://github.com", manual_proxy);
            let checksums = download(
                proxy.as_ref(),
                checksums_url,
                MAX_CHECKSUM_BYTES,
                &|_, _| {},
            )
            .await
            .context("downloading checksums.txt")?;
            if on_progress(0, None).is_break() {
                bail!(CANCELLED);
            }
            let archive = download(
                proxy.as_ref(),
                asset_url,
                MAX_ASSET_BYTES,
                &|received, total| {
                    progress.set((received, total));
                },
            )
            .await
            .context("downloading the update package")?;
            if on_progress(progress.get().0, progress.get().1).is_break() {
                bail!(CANCELLED);
            }
            Ok((checksums, archive))
        }
        .or(async {
            loop {
                let (received, total) = progress.get();
                if on_progress(received, total).is_break() {
                    bail!(CANCELLED);
                }
                smol::Timer::after(CANCEL_POLL).await;
            }
        }),
    )
}

async fn download(
    proxy: Option<&Proxy>,
    url: &str,
    limit: usize,
    progress: &dyn Fn(u64, Option<u64>),
) -> Result<Vec<u8>> {
    async {
        let mut url = reqwest::Url::parse(url).context("parsing the release URL")?;
        let mut redirects = 0;
        let mut response = loop {
            let uri = url
                .as_str()
                .parse()
                .context("parsing the release request URL")?;
            let route = proxy.filter(|proxy| !proxy.is_no_proxy(&uri));
            let client = client(route).await?;
            let request = Request::get(url.as_str())
                .follow_redirects(RedirectPolicy::NoFollow)
                .body(AsyncBody::default())?;
            let response = client.send(request).await?;
            if matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308)
                && let Some(location) = response.headers().get("location")
            {
                if redirects == 10 {
                    bail!("the release server returned too many redirects");
                }
                url = url
                    .join(location.to_str().context("reading the release redirect")?)
                    .context("parsing the release redirect")?;
                redirects += 1;
                continue;
            }
            break response;
        };
        if !response.status().is_success() {
            bail!(
                "the release server returned HTTP {}",
                response.status().as_u16()
            );
        }
        let declared = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        if declared.is_some_and(|length| length > limit as u64) {
            bail!("the download exceeds the {limit} byte limit");
        }
        let mut bytes = Vec::with_capacity(declared.unwrap_or(0) as usize);
        let mut chunk = [0; 64 * 1024];
        loop {
            let n = response.body_mut().read(&mut chunk).await?;
            if n == 0 {
                return Ok(bytes);
            }
            if bytes.len().saturating_add(n) > limit {
                bail!("the download exceeds the {limit} byte limit");
            }
            bytes.extend_from_slice(&chunk[..n]);
            progress(bytes.len() as u64, declared);
        }
    }
    .or(async {
        smol::Timer::after(DOWNLOAD_TIMEOUT).await;
        bail!("the download did not finish within 15 minutes")
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead as _, Read as _, Write as _};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::time::Instant;

    fn listener() -> (TcpListener, String) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let proxy = format!("http://{}", listener.local_addr().unwrap());
        (listener, proxy)
    }

    fn accept(listener: &TcpListener) -> TcpStream {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    return stream;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "the client made no request");
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        }
    }

    fn request(stream: &TcpStream) -> String {
        let mut reader = std::io::BufReader::new(stream);
        let mut headers = String::new();
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).unwrap() > 0);
            headers.push_str(&line);
            if line == "\r\n" {
                return headers;
            }
        }
    }

    fn respond(stream: &mut TcpStream, body: &[u8]) {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
    }

    fn redirect(stream: &mut TcpStream, location: &str) {
        write!(stream, "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    }

    #[derive(Clone, Copy)]
    enum Stall {
        Headers,
        ChecksumsBody,
        AssetBody,
        Tls,
    }

    fn assert_stalled_request_cancels(stall: Stall) {
        let (listener, proxy) = listener();
        let requested = Arc::new(AtomicBool::new(false));
        let seen = requested.clone();
        let server = std::thread::spawn(move || {
            if matches!(stall, Stall::AssetBody) {
                let mut checksums = accept(&listener);
                assert!(request(&checksums).contains("/checksums.txt"));
                respond(&mut checksums, b"manifest");
            }
            let mut stream = accept(&listener);
            let headers = request(&stream);
            match stall {
                Stall::Headers => {}
                Stall::ChecksumsBody | Stall::AssetBody => {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n",
                        )
                        .unwrap();
                }
                Stall::Tls => {
                    assert!(headers.starts_with("CONNECT "));
                    stream
                        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                        .unwrap();
                }
            }
            let cancelled_at = Instant::now();
            seen.store(true, Ordering::Release);
            // The server observes the closed connection. Returning early while
            // leaving a blocking download in another thread would fail this.
            let mut bytes = [0; 4096];
            loop {
                match stream.read(&mut bytes) {
                    Ok(0) => return (true, cancelled_at),
                    Ok(_) => {}
                    Err(error) => {
                        return (
                            matches!(
                                error.kind(),
                                std::io::ErrorKind::ConnectionReset
                                    | std::io::ErrorKind::ConnectionAborted
                            ),
                            cancelled_at,
                        );
                    }
                }
            }
        });
        let checksums_url = if matches!(stall, Stall::Tls) {
            "https://update.test/checksums.txt"
        } else {
            "http://update.test/checksums.txt"
        };
        let error = fetch(
            Some(&proxy),
            checksums_url,
            "http://update.test/package.zip",
            &|_, _| {
                if requested.load(Ordering::Acquire) {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        )
        .unwrap_err();
        let returned_at = Instant::now();
        let (closed, cancelled_at) = server.join().unwrap();
        let elapsed = returned_at.saturating_duration_since(cancelled_at);
        assert!(format!("{error:#}").contains(CANCELLED), "{error:#}");
        assert!(elapsed < Duration::from_secs(1), "cancel took {elapsed:?}");
        assert!(closed, "cancelling must close the stalled connection");
    }

    #[test]
    fn cancellation_interrupts_stalled_checksums_before_headers() {
        assert_stalled_request_cancels(Stall::Headers);
    }

    #[test]
    fn cancellation_interrupts_stalled_checksums_body() {
        assert_stalled_request_cancels(Stall::ChecksumsBody);
    }

    #[test]
    fn cancellation_interrupts_a_stalled_asset_body() {
        assert_stalled_request_cancels(Stall::AssetBody);
    }

    #[test]
    fn cancellation_interrupts_the_tls_handshake() {
        assert_stalled_request_cancels(Stall::Tls);
    }

    #[test]
    fn a_cancelled_attempt_makes_no_network_request() {
        let (listener, proxy) = listener();
        let (done_tx, done_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if let Ok((mut stream, _)) = listener.accept() {
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    return true;
                }
                if done_rx.try_recv().is_ok() || Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let error = fetch(
            Some(&proxy),
            "http://update.test/checksums.txt",
            "http://update.test/package.zip",
            &|_, _| ControlFlow::Break(()),
        )
        .unwrap_err();
        let _ = done_tx.send(());
        assert!(
            !server.join().unwrap(),
            "a cancelled attempt still contacted the server"
        );
        assert!(format!("{error:#}").contains(CANCELLED), "{error:#}");
    }

    #[test]
    fn an_unresolved_socks_proxy_never_falls_back_to_a_direct_request() {
        let (listener, endpoint) = listener();
        let (done_tx, done_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            loop {
                if let Ok((mut stream, _)) = listener.accept() {
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    return true;
                }
                if done_rx.try_recv().is_ok() {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let result = fetch(
            Some("socks5://unresolvable.proxy.invalid:1080"),
            &format!("{endpoint}/checksums.txt"),
            &format!("{endpoint}/package.zip"),
            &|_, _| ControlFlow::Continue(()),
        );
        let _ = done_tx.send(());
        assert!(
            !server.join().unwrap(),
            "a failed configured proxy must not cause a direct request"
        );
        assert!(result.is_err());
    }

    #[test]
    fn an_unresolvable_bypassed_proxy_does_not_block_the_download() {
        let (listener, endpoint) = listener();
        let selected_proxy = Proxy::builder(proxy::ProxyProtocol::Socks5)
            .host("unresolvable.proxy.invalid")
            .port(1080)
            .no_proxy("127.0.0.1")
            .build()
            .unwrap();
        let (done_tx, done_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            loop {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    request(&stream);
                    respond(&mut stream, b"direct package");
                    return;
                }
                if done_rx.try_recv().is_ok() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let result = smol::block_on(download(Some(&selected_proxy), &endpoint, 1024, &|_, _| {}));
        let _ = done_tx.send(());
        server.join().unwrap();
        assert_eq!(result.unwrap(), b"direct package");
    }

    #[test]
    fn a_redirect_to_a_non_bypassed_host_requires_the_configured_proxy() {
        let (listener, endpoint) = listener();
        let location = format!(
            "http://localhost:{}/package",
            listener.local_addr().unwrap().port()
        );
        let selected_proxy = Proxy::builder(proxy::ProxyProtocol::Socks5)
            .host("unresolvable.proxy.invalid")
            .port(1080)
            .no_proxy("127.0.0.1")
            .build()
            .unwrap();
        let (done_tx, done_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let mut first = accept(&listener);
            request(&first);
            redirect(&mut first, &location);
            loop {
                if let Ok((mut stream, _)) = listener.accept() {
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    return true;
                }
                if done_rx.try_recv().is_ok() {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let result = smol::block_on(download(Some(&selected_proxy), &endpoint, 1024, &|_, _| {}));
        let _ = done_tx.send(());
        assert!(
            !server.join().unwrap(),
            "the redirect bypassed a required proxy"
        );
        assert!(format!("{:#}", result.unwrap_err()).contains("SOCKS update proxy"));
    }

    #[test]
    fn a_redirect_can_switch_from_the_proxy_to_a_bypassed_host() {
        let (proxy_listener, _) = listener();
        let (direct_listener, endpoint) = listener();
        let selected_proxy = Proxy::builder(proxy::ProxyProtocol::Http)
            .host("127.0.0.1")
            .port(proxy_listener.local_addr().unwrap().port())
            .no_proxy("127.0.0.1")
            .build()
            .unwrap();
        let server = std::thread::spawn(move || {
            let mut proxied = accept(&proxy_listener);
            assert!(request(&proxied).starts_with("GET http://update.test/start "));
            redirect(&mut proxied, &endpoint);
            let mut direct = accept(&direct_listener);
            assert!(request(&direct).starts_with("GET / HTTP/1.1"));
            respond(&mut direct, b"redirected package");
        });
        let result = smol::block_on(download(
            Some(&selected_proxy),
            "http://update.test/start",
            1024,
            &|_, _| {},
        ));
        server.join().unwrap();
        assert_eq!(result.unwrap(), b"redirected package");
    }

    #[test]
    fn relative_redirect_loops_stop_at_the_limit() {
        let (listener, endpoint) = listener();
        let server = std::thread::spawn(move || {
            for _ in 0..=10 {
                let mut stream = accept(&listener);
                request(&stream);
                redirect(&mut stream, "/again");
            }
        });
        let result = smol::block_on(download(None, &endpoint, 1024, &|_, _| {}));
        server.join().unwrap();
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("too many redirects")
        );
    }

    #[test]
    fn completed_downloads_preserve_bytes_and_report_asset_progress() {
        let (listener, proxy) = listener();
        let server = std::thread::spawn(move || {
            for (path, bytes) in [
                ("/checksums.txt", b"manifest".as_slice()),
                ("/package.zip", b"package".as_slice()),
            ] {
                let mut stream = accept(&listener);
                assert!(request(&stream).starts_with(&format!("GET http://update.test{path} ")));
                respond(&mut stream, bytes);
            }
        });
        let progress = Cell::new((0, None));
        let result = fetch(
            Some(&proxy),
            "http://update.test/checksums.txt",
            "http://update.test/package.zip",
            &|received, total| {
                progress.set((received, total));
                ControlFlow::Continue(())
            },
        );
        server.join().unwrap();
        let (checksums, package) = result.unwrap();
        assert_eq!(checksums, b"manifest");
        assert_eq!(package, b"package");
        assert_eq!(progress.get(), (7, Some(7)));
    }

    #[test]
    fn size_limits_apply_with_and_without_content_length() {
        for declared in [true, false] {
            let (listener, proxy) = listener();
            let server = std::thread::spawn(move || {
                let mut stream = accept(&listener);
                request(&stream);
                if declared {
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n",
                    );
                } else {
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n12345");
                }
            });
            let error = smol::block_on(download(
                proxy::resolve("https://github.com", Some(&proxy)).as_ref(),
                "http://update.test/package.zip",
                4,
                &|_, _| {},
            ))
            .unwrap_err();
            server.join().unwrap();
            assert!(error.to_string().contains("4 byte limit"), "{error}");
        }
    }
}
