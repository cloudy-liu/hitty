//! Opt-in acceptance at the production document panel with a real system WebView.
//! Browser observations use evaluation callbacks, never a page-to-app IPC bridge.

use gpui::{prelude::*, *};
use gpui_component::Root;
use serde::Deserialize;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use super::*;
use crate::core::config::Config;

const FOLLOW_FRAGMENT: &str = "(() => { const target = document.createElement('span'); target.id = 'reading-position'; target.style = 'position:absolute;top:600px;left:0;width:1px;height:1px'; const link = document.createElement('a'); link.href = '#reading-position'; document.body.append(target, link); link.click(); return location.hash; })()";

#[derive(Deserialize)]
struct Options {
    source: PathBuf,
    output: PathBuf,
    #[serde(default)]
    hold_seconds: Option<u64>,
}

pub(crate) fn run(path: &std::ffi::OsStr) {
    let options: Options = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    gpui_platform::application()
        .with_assets(crate::ui::assets::Assets)
        .with_quit_mode(QuitMode::Explicit)
        .run(move |cx| {
            gpui_component::init(cx);
            crate::register_bundled_fonts(cx);
            let mut config = Config::default();
            config.theme_follow_system = false;
            config.theme_preset = "light".into();
            config.install_cli_on_path = false;
            cx.set_global(config);
            crate::core::session::WorkspaceStore::init(cx);
            crate::ui::windows::WindowRegistry::init(cx);
            crate::ui::presets::load_registry(cx);
            crate::ui::keymap::init(cx);
            let mut owner = None;
            let handle = cx.open_window(WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None, size(px(1200.), px(900.)), cx))),
                titlebar: Some(TitlebarOptions {
                    title: Some("HTML preview acceptance".into()), ..Default::default()
                }),
                ..Default::default()
            }, |window, cx| {
                crate::ui::theme::apply_theme(Some(window), cx);
                let app = cx.new(|cx| Tty7App::with_session(None,
                    Some(crate::core::session::Session::default()), window, cx));
                app.update(cx, |app, cx| {
                    app.tabs.push(crate::ui::app::Tab::new(crate::ui::pane::Pane::Empty));
                    app.active = app.tabs.len() - 1;
                    app.editor_open_on_host(tty7_core::host::local::LocalHost::new(),
                        &options.source, window, cx);
                });
                owner = Some(app.clone());
                cx.new(|cx| Root::new(app, window, cx))
            }).unwrap();
            cx.activate(true);
            let app = owner.unwrap();
            cx.spawn(async move |cx| {
                let started = Instant::now();
                let mut samples = Vec::new();
                let deadline = Instant::now() + Duration::from_secs(45);
                loop {
                    assert!(Instant::now() < deadline, "HTML resources did not load");
                    cx.background_executor().timer(Duration::from_millis(100)).await;
                    let view = app.read_with(cx, |app, _| app.editor.html.clone());
                    if let Some(view) = view {
                        let rx = view.read_with(cx, |view, cx| view.inspect(
                            "JSON.stringify({ready: document.querySelector('#resources')?.textContent === 'All loaded', image: [...document.images].every(i => i.complete && i.naturalWidth > 0), grid: getComputedStyle(document.querySelector('.metrics')).display, url: location.href, userAgent: navigator.userAgent, width: innerWidth, height: innerHeight, dpr: devicePixelRatio})", cx)).unwrap();
                        let Ok(json) = rx.recv().await else { continue; };
                        // WRY serializes the JavaScript string result once more.
                        let Ok(json) = serde_json::from_str::<String>(&json) else { continue; };
                        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
                        if value["ready"] == true && value["image"] == true {
                            assert_eq!(value["grid"], "grid");
                            assert!(value["width"].as_f64().unwrap() > 100.);
                            assert!(value["height"].as_f64().unwrap() > 100.);
                            samples.push(value);
                            break;
                        }
                    }
                    assert!(Instant::now() < deadline, "HTML resources did not load");
                }
                let cold_load_ms = started.elapsed().as_secs_f64() * 1000.;
                let view = app.read_with(cx, |app, _| app.editor.html.clone().unwrap());
                let rx = view.read_with(cx, |view, cx| view.inspect(
                    "document.querySelector('#counter').click(); document.querySelector('#count').textContent", cx)).unwrap();
                assert_eq!(rx.recv().await.unwrap(), "\"1\"", "relative JavaScript must handle the click");

                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.editor_refresh_html(window, cx);
                })).unwrap();
                let deadline = Instant::now() + Duration::from_secs(15);
                let refresh_started = Instant::now();
                loop {
                    cx.background_executor().timer(Duration::from_millis(100)).await;
                    let rx = view.read_with(cx, |view, cx| view.inspect(
                        "document.querySelector('#count')?.textContent", cx)).unwrap();
                    if rx.recv().await.unwrap() == "\"0\"" { break; }
                    assert!(Instant::now() < deadline, "refresh did not reset the page");
                }
                let refresh_ms = refresh_started.elapsed().as_secs_f64() * 1000.;
                wait_for_scroll(&app, 0., cx).await;
                let rx = view.read_with(cx, |view, cx| view.inspect(
                    FOLLOW_FRAGMENT, cx)).unwrap();
                assert_eq!(rx.recv().await.unwrap(), "\"#reading-position\"",
                    "same-document links must retain the HTML preview");
                let started = Instant::now();
                loop {
                    let rx = view.read_with(cx, |view, cx| view.inspect(
                        "window.scrollTo(0, 600); window.scrollY", cx)).unwrap();
                    if rx.recv().await.unwrap() == "600" { break; }
                    assert!(started.elapsed() < Duration::from_secs(5), "fixture must have a readable scroll position");
                    cx.background_executor().timer(Duration::from_millis(50)).await;
                }
                let mut switch_ms = Vec::new();
                for (path, title) in [
                    (options.source.parent().unwrap().join("next.htm"), "\"Second HTML preview\""),
                    (options.source.clone(), "\"HTML preview acceptance\""),
                ] {
                    let started = Instant::now();
                    cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                        app.editor_open_on_host(tty7_core::host::local::LocalHost::new(), &path, window, cx);
                    })).unwrap();
                    loop {
                        assert!(started.elapsed() < Duration::from_secs(15), "file switch did not load");
                        cx.background_executor().timer(Duration::from_millis(100)).await;
                        let rx = view.read_with(cx, |view, cx| view.inspect("document.title", cx)).unwrap();
                        if rx.recv().await.unwrap_or_default() == title { break; }
                    }
                    assert_eq!(app.read_with(cx, |app, _| app.editor.html.as_ref().unwrap().entity_id()), view.entity_id(),
                        "switching files must reuse the browser");
                    switch_ms.push(started.elapsed().as_secs_f64() * 1000.);
                }
                let started = Instant::now();
                loop {
                    let rx = view.read_with(cx, |view, cx| view.inspect("window.scrollY", cx)).unwrap();
                    if rx.recv().await.unwrap() == "600" { break; }
                    assert!(started.elapsed() < Duration::from_secs(5), "HTML reading position was lost across file tabs");
                    cx.background_executor().timer(Duration::from_millis(50)).await;
                }
                let rx = view.read_with(cx, |view, cx| view.inspect(FOLLOW_FRAGMENT, cx)).unwrap();
                assert_eq!(rx.recv().await.unwrap(), "\"#reading-position\"");
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.editor_refresh_html(window, cx);
                })).unwrap();
                wait_for_scroll(&app, 0., cx).await;
                // Exercise the same panel boundary when the singleton is
                // released, rather than only navigating an existing browser.
                let mut markdown = tempfile::Builder::new().suffix(".md").tempfile().unwrap();
                use std::io::Write as _;
                markdown.write_all(b"# Another document\nReading Markdown between HTML files.").unwrap();
                let rx = view.read_with(cx, |view, cx| view.inspect("window.scrollTo(0,850); scrollY", cx)).unwrap();
                assert!((serde_json::from_str::<f64>(&rx.recv().await.unwrap()).unwrap() - 850.).abs() < 1.);
                let released = view.downgrade();
                drop(view);
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.editor_open_on_host(tty7_core::host::local::LocalHost::new(), markdown.path(), window, cx);
                })).unwrap();
                wait_for_release(released, cx).await;
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.editor_open_on_host(tty7_core::host::local::LocalHost::new(), &options.source, window, cx);
                })).unwrap();
                let view = wait_for_scroll(&app, 850., cx).await;
                let released = view.downgrade();
                drop(view);
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.editor_toggle_preview(window, cx);
                    assert!(!app.tab_code().unwrap().active_file().unwrap().preview);
                    assert!(app.editor.html.is_none());
                })).unwrap();
                wait_for_release(released, cx).await;
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.editor_toggle_preview(window, cx);
                })).unwrap();
                let view = wait_for_scroll(&app, 850., cx).await;
                let singleton = view.entity_id();
                let original_tab = app.read_with(cx, |app, _| app.active);
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.tabs.push(crate::ui::app::Tab::new(crate::ui::pane::Pane::Empty));
                    app.active = app.tabs.len() - 1;
                    app.editor_open_on_host(tty7_core::host::local::LocalHost::new(), &options.source, window, cx);
                })).unwrap();
                let second = wait_for_scroll(&app, 0., cx).await;
                assert_eq!(second.entity_id(), singleton, "same-path session changes still reuse one browser");
                let rx = second.read_with(cx, |view, cx| view.inspect("window.scrollTo(0,250); scrollY", cx)).unwrap();
                assert!((serde_json::from_str::<f64>(&rx.recv().await.unwrap()).unwrap() - 250.).abs() < 1.);
                drop(second);
                let second_tab = app.read_with(cx, |app, _| app.active);
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.active = original_tab;
                    app.editor_sync_html_preview(true, window, cx);
                })).unwrap();
                wait_for_scroll(&app, 850., cx).await;
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.active = second_tab;
                    app.editor_sync_html_preview(true, window, cx);
                })).unwrap();
                wait_for_scroll(&app, 250., cx).await;
                // Both selections happen before the outgoing script callback.
                // Only the latest owner should be activated after capture.
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.active = original_tab;
                    app.editor_sync_html_preview(true, window, cx);
                    app.active = second_tab;
                    app.editor_sync_html_preview(true, window, cx);
                })).unwrap();
                assert_eq!(wait_for_scroll(&app, 250., cx).await.entity_id(), singleton);
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    let ix = app.tab_code().unwrap().active;
                    app.editor_close_file(ix, window, cx);
                    app.editor_open_on_host(tty7_core::host::local::LocalHost::new(), &options.source, window, cx);
                })).unwrap();
                wait_for_scroll(&app, 0., cx).await;
                drop(view);
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.tabs.remove(second_tab);
                    app.active = original_tab;
                    app.editor_sync_html_preview(true, window, cx);
                })).unwrap();
                let view = wait_for_scroll(&app, 850., cx).await;
                let rx = view.read_with(cx, |view, cx| view.inspect("window.scrollTo(0,0); scrollY", cx)).unwrap();
                assert_eq!(rx.recv().await.unwrap(), "0");
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.open_palette("theme", window, cx);
                    app.editor_sync_html_preview(false, window, cx);
                })).unwrap();
                cx.background_executor().timer(Duration::from_millis(500)).await;
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.close_palette(window, cx);
                    app.editor_sync_html_preview(true, window, cx);
                })).unwrap();
                cx.background_executor().timer(Duration::from_secs(2)).await;
                let mut evidence = serde_json::json!({
                    "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
                    "cold_load_poll_ms": cold_load_ms, "refresh_poll_ms": refresh_ms,
                    "switch_poll_ms": switch_ms, "samples": samples,
                    "script_click": true, "refresh": true,
                    "html_tab_scroll": true, "fragment_scroll_and_refresh": true,
                    "markdown_and_source_scroll": true,
                    "session_scroll_ownership": true, "rapid_switch_and_reopen": true,
                    "note": "DOM polling includes resource readiness; it does not measure first visible paint. Screenshot must be inspected separately."
                });
                eprintln!("NATIVE_HTML_READY_FOR_CAPTURE");
                cx.background_executor().timer(Duration::from_secs(options.hold_seconds.unwrap_or(4))).await;
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.editor_toggle_preview(window, cx);
                    assert!(!app.tab_code().unwrap().active_file().unwrap().preview);
                    assert!(app.editor.html.is_none());
                })).unwrap();
                drop(view);
                // Delete only a fixture-owned temporary file, never the input
                // document. Navigation begins successfully and fails later.
                let mut fault = tempfile::Builder::new().suffix(".html").tempfile().unwrap();
                let saved = "<!doctype html><title>Failure recovery</title><p>Saved buffer</p>";
                fault.write_all(saved.as_bytes()).unwrap();
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.editor_open_on_host(tty7_core::host::local::LocalHost::new(), fault.path(), window, cx);
                })).unwrap();
                let started = Instant::now();
                loop {
                    assert!(started.elapsed() < Duration::from_secs(15), "fault fixture did not load");
                    cx.background_executor().timer(Duration::from_millis(100)).await;
                    let view = app.read_with(cx, |app, _| app.editor.html.clone());
                    if let Some(view) = view {
                        let rx = view.read_with(cx, |view, cx| view.inspect("document.title", cx)).unwrap();
                        if rx.recv().await.unwrap_or_default() == "\"Failure recovery\"" { break; }
                    }
                }
                fault.close().unwrap();
                cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                    app.editor_refresh_html(window, cx);
                })).unwrap();
                let started = Instant::now();
                loop {
                    assert!(started.elapsed() < Duration::from_secs(15), "asynchronous failure did not restore source");
                    cx.background_executor().timer(Duration::from_millis(100)).await;
                    if app.read_with(cx, |app, cx| {
                        let file = app.tab_code().unwrap().active_file().unwrap();
                        !file.preview && app.editor.html.is_none() && file.input.read(cx).text().to_string() == saved
                    }) { break; }
                }
                evidence["navigation_failure_recovery"] = true.into();
                std::fs::write(&options.output, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
                eprintln!("NATIVE_HTML_SOURCE_RESTORED");
                cx.background_executor().timer(Duration::from_secs(10)).await;
                eprintln!("NATIVE_HTML_PASSED");
                cx.update_window(handle.into(), |_, window, _| window.remove_window()).unwrap();
                cx.update(|cx| cx.quit());
            }).detach();
        });
}

async fn wait_for_scroll(
    app: &Entity<Tty7App>,
    expected: f64,
    cx: &mut AsyncApp,
) -> Entity<crate::ui::html_preview::HtmlPreview> {
    let started = Instant::now();
    loop {
        if let Some(view) = app.read_with(cx, |app, _| app.editor.html.clone())
            && view.read_with(cx, |view, _| view.ready())
        {
            let rx = view
                .read_with(cx, |view, cx| view.inspect("scrollY", cx))
                .unwrap();
            if rx
                .recv()
                .await
                .ok()
                .and_then(|value| serde_json::from_str::<f64>(&value).ok())
                .is_some_and(|actual| (actual - expected).abs() < 1.)
            {
                return view;
            }
        }
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "HTML reading position {expected} was not restored"
        );
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
    }
}

async fn wait_for_release(
    view: WeakEntity<crate::ui::html_preview::HtmlPreview>,
    cx: &mut AsyncApp,
) {
    let started = Instant::now();
    while view.upgrade().is_some() {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "leaving preview retained its browser"
        );
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
    }
}
