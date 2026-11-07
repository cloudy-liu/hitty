//! Opt-in native visual fixture using the production Markdown reading view.

use gpui::{prelude::*, *};
use gpui_component::{Root, input::InputState};
use serde::Deserialize;
use std::{path::PathBuf, time::Duration};

use crate::{
    core::config::Config,
    ui::{app::Tty7App, markdown_preview::MarkdownPreview},
};

#[derive(Deserialize)]
struct Options {
    source: PathBuf,
    width: f32,
    height: f32,
    scale: f32,
    dark: bool,
    #[serde(default)]
    anchor: String,
    #[serde(default)]
    output: Option<PathBuf>,
    #[serde(default)]
    profile_output: Option<PathBuf>,
    #[serde(default = "default_hold_seconds")]
    hold_seconds: u64,
}

fn default_hold_seconds() -> u64 {
    12
}

struct Fixture {
    reading: Entity<MarkdownPreview>,
    _owner: Entity<Tty7App>,
    scale: f32,
}

impl Render for Fixture {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(px(16. * self.scale));
        div()
            .flex()
            .w(window.viewport_size().width)
            .h(window.viewport_size().height)
            .overflow_hidden()
            .child(AnyView::from(self.reading.clone()).cached(div().size_full().style().clone()))
            .children(Root::render_dialog_layer(window, cx))
    }
}

pub(crate) fn run(path: &std::ffi::OsStr) {
    eprintln!("NATIVE_MARKDOWN_START os={}", std::env::consts::OS);
    let options: Options = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!((400. ..=1600.).contains(&options.width));
    assert!((400. ..=1600.).contains(&options.height));
    assert!((1. ..=2.).contains(&options.scale));
    let source = std::fs::read_to_string(&options.source).unwrap();
    let source_bytes = source.len();
    let started = std::time::Instant::now();
    gpui_platform::application().with_assets(crate::ui::assets::Assets)
        .with_quit_mode(QuitMode::Explicit).run(move |cx| {
            eprintln!("NATIVE_MARKDOWN_APPLICATION_READY");
            gpui_component::init(cx);
            eprintln!("NATIVE_MARKDOWN_COMPONENTS_READY");
            crate::register_bundled_fonts(cx);
            eprintln!("NATIVE_MARKDOWN_FONTS_READY");
            let mut config = Config::default();
            config.theme_follow_system = false;
            config.theme_preset = if options.dark { "dark" } else { "light" }.into();
            config.install_cli_on_path = false;
            cx.set_global(config);
            crate::core::session::WorkspaceStore::init(cx);
            crate::ui::windows::WindowRegistry::init(cx);
            crate::ui::presets::load_registry(cx);
            crate::ui::keymap::init(cx);
            crate::ui::markdown_preview::init(Default::default(), cx);
            let mut reading = None;
            eprintln!("NATIVE_MARKDOWN_OPENING_WINDOW");
            let handle = cx.open_window(WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None, size(px(options.width), px(options.height)), cx))),
                titlebar: Some(TitlebarOptions { title: Some("Markdown visual acceptance".into()), ..Default::default() }),
                show: true,
                focus: true,
                ..Default::default()
            }, |window, cx| {
                eprintln!("NATIVE_MARKDOWN_APPLYING_THEME");
                crate::ui::theme::apply_theme(Some(window), cx);
                eprintln!("NATIVE_MARKDOWN_CREATING_OWNER");
                let owner = cx.new(|cx| Tty7App::with_session(None,
                    Some(crate::core::session::Session::default()), window, cx));
                eprintln!("NATIVE_MARKDOWN_OWNER_READY");
                window.set_window_title("Markdown visual acceptance");
                let input = cx.new(|cx| {
                    let mut input = InputState::new(window, cx).multi_line(true);
                    input.set_value(source, window, cx);
                    input
                });
                let view = cx.new(|cx| MarkdownPreview::new(input,
                    tty7_core::host::local::LocalHost::new(), options.source.clone(),
                    owner.downgrade(), true, cx));
                window.focus(&view.focus_handle(cx), cx);
                reading = Some(view.clone());
                let fixture = cx.new(|_| Fixture { reading: view, _owner: owner, scale: options.scale });
                cx.new(|cx| Root::new(fixture, window, cx))
            }).unwrap();
            cx.activate(true);
            let reading = reading.unwrap();
            cx.spawn(async move |cx| {
                // Real executors and native font shaping are retained here.
                // Wait for the production background parser rather than a fixed screenshot delay.
                let deadline = std::time::Instant::now() + Duration::from_secs(20);
                loop {
                    cx.background_executor().timer(Duration::from_millis(100)).await;
                    let ready = reading.read_with(cx, |view, cx| !view.is_loading(cx)
                        && !view.pending_diagrams_for_profile());
                    if ready { break; }
                    assert!(std::time::Instant::now() < deadline, "Markdown parse timed out");
                }
                let content_ready_ms = started.elapsed().as_secs_f64() * 1000.;
                cx.background_executor().timer(Duration::from_secs(2)).await;
                // Navigate after the bounded viewport and scroll extent have been laid out.
                // This fixture starts a fresh native window for every requested section.
                if !options.anchor.is_empty() {
                    reading.update(cx, |view, cx| view.navigate_anchor(options.anchor.clone(), cx));
                    cx.background_executor().timer(Duration::from_secs(2)).await;
                }
                cx.update_window(handle.into(), |_, window, cx| {
                    window.refresh();
                    let _ = window.draw(cx);
                    window.refresh();
                }).unwrap();
                cx.background_executor().timer(Duration::from_millis(200)).await;
                cx.update_window(handle.into(), |_, window, cx| {
                    eprintln!("native Markdown: os={}, dpi_scale={}, app_scale={}, viewport={:?}, ready={:?}",
                        std::env::consts::OS, window.scale_factor(), options.scale,
                        window.viewport_size(), started.elapsed());
                    let view = reading.read(cx);
                    assert!(!view.text.read(cx).source().is_empty());
                    if !options.anchor.is_empty() {
                        let target = view.text.read(cx).anchor_bounds(&options.anchor)
                            .expect("requested heading must be laid out");
                        let viewport = view.scroll.bounds();
                        eprintln!("native anchor: requested={}, offset={:?}, target={:?}, viewport={:?}",
                            options.anchor, view.profile_scroll_offset(cx), target, viewport);
                        assert!((viewport.size.height - window.viewport_size().height).abs() < px(2.),
                            "Markdown scroll container must fit the native window");
                        assert!(view.profile_scroll_offset(cx).y < px(-100.)
                            && target.top() < viewport.bottom() && target.bottom() > viewport.top(),
                            "requested Markdown heading is outside the captured viewport");
                    }
                    if let Some(output) = &options.output {
                        #[cfg(target_os = "macos")]
                        {
                            let screenshot = window.render_to_image().unwrap();
                            let (min, max) = screenshot.pixels()
                                .map(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]))
                                .fold((765, 0), |(min, max), value| (min.min(value), max.max(value)));
                            assert!(max - min > 96, "native Markdown screenshot has no visible content");
                            screenshot.save(output).unwrap();
                        }
                        #[cfg(not(target_os = "macos"))]
                        let _ = output;
                    }
                }).unwrap();
                eprintln!("NATIVE_MARKDOWN_READY");
                if let Some(output) = &options.profile_output {
                    let mut scroll_frames = Vec::new();
                    let mut scroll_detail = Vec::new();
                    let mut offsets = Vec::new();
                    for frame in 0..110 {
                        cx.update_window(handle.into(), |_, window, cx| {
                            // Use the same outer-pane input position as the baseline,
                            // including when the reading view forwards margin wheel events.
                            let viewport = window.viewport_size();
                            let position = point(viewport.width - px(12.), viewport.height / 2.);
                            let start = std::time::Instant::now();
                            window.dispatch_event(PlatformInput::ScrollWheel(ScrollWheelEvent {
                                position,
                                delta: ScrollDelta::Pixels(point(px(0.), px(if frame < 55 { -120. } else { 120. }))),
                                modifiers: Modifiers::none(), touch_phase: TouchPhase::Moved,
                            }), cx);
                            let _ = window.draw(cx);
                            let elapsed = start.elapsed().as_secs_f64() * 1000.;
                            reading.read(cx).verify_profile_scroll_frame(cx);
                            if frame >= 10 { scroll_frames.push(elapsed); }
                            scroll_detail.push(serde_json::json!({"frame":frame,"cpu_ms":elapsed,
                                "offset":f32::from(reading.read(cx).profile_scroll_offset(cx).y)}));
                            offsets.push(f32::from(reading.read(cx).profile_scroll_offset(cx).y));
                        }).unwrap();
                    }
                    assert!(offsets.iter().copied().reduce(f32::min).unwrap() < -500., "profile must actually scroll");
                    let viewer = cx.update_window(handle.into(), |_, window, cx| {
                        let (image, code, aspect) = reading.read(cx).diagram_for_profile(1, cx);
                        if let ImageSource::Image(data) = &image {
                            std::fs::write(output.with_extension("svg"), &data.bytes).unwrap();
                        }
                        let viewer = crate::ui::markdown_mermaid::open_diagram(image, code, aspect, window, cx);
                        viewer.update(cx, |viewer, cx| viewer.prepare_profile(cx));
                        window.refresh();
                        let _ = window.draw(cx);
                        viewer
                    }).unwrap();
                    cx.background_executor().timer(Duration::from_millis(400)).await;
                    let mut drag_frames = Vec::new();
                    cx.update_window(handle.into(), |_, window, cx| {
                        // A hidden native window does not receive the idle draws
                        // that normally finish dialog layout. Resolve its size
                        // before positioning the benchmark away from scroll edges.
                        window.refresh();
                        let _ = window.draw(cx);
                        viewer.update(cx, |viewer, cx| viewer.prepare_profile(cx));
                        window.refresh();
                        let _ = window.draw(cx);
                        let position = viewer.read(cx).profile_bounds().center();
                        window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent {
                            position, button: MouseButton::Left, modifiers: Modifiers::none(), click_count: 1, first_mouse: false,
                        }), cx);
                    }).unwrap();
                    for frame in 0..110 {
                        cx.update_window(handle.into(), |_, window, cx| {
                            let position = viewer.read(cx).profile_bounds().center() + point(px(0.), px(if frame % 2 == 0 { -24. } else { 24. }));
                            let start = std::time::Instant::now();
                            let previous = viewer.read(cx).profile_pan();
                            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent {
                                position, pressed_button: Some(MouseButton::Left), modifiers: Modifiers::none(),
                            }), cx);
                            let _ = window.draw(cx);
                            viewer.read(cx).verify_profile_frame();
                            assert_ne!(viewer.read(cx).profile_pan(), previous, "profile drag must move the diagram");
                            if frame >= 10 { drag_frames.push(start.elapsed().as_secs_f64() * 1000.); }
                        }).unwrap();
                    }
                    let (dpi_scale, viewport) = cx.update_window(handle.into(), |_, window, cx| {
                        window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent {
                            position: viewer.read(cx).profile_bounds().center(), button: MouseButton::Left, modifiers: Modifiers::none(), click_count: 1,
                        }), cx);
                        (window.scale_factor(), window.viewport_size())
                    }).unwrap();
                    let profile = serde_json::json!({
                        "os": std::env::consts::OS, "dpi_scale": dpi_scale,
                        "window": [f32::from(viewport.width), f32::from(viewport.height)],
                        "source_bytes": source_bytes,
                        "parsed_source_bytes": reading.read_with(cx, |view, cx| view.text.read(cx).source().len()),
                        "all_content_ready_ms": content_ready_ms,
                        "scroll_cpu_frame": frame_summary(&mut scroll_frames),
                        "scroll_detail": scroll_detail,
                        "expanded_drag_cpu_frame": frame_summary(&mut drag_frames),
                        "min_scroll_offset": offsets.iter().copied().reduce(f32::min).unwrap(),
                        "visible_scroll_verified": true,
                        "measurement": "event dispatch plus GPUI CPU layout/prepaint/paint; excludes present and GPU time",
                    });
                    std::fs::write(output, serde_json::to_vec_pretty(&profile).unwrap()).unwrap();
                    eprintln!("NATIVE_MARKDOWN_PROFILE {profile}");
                }
                cx.background_executor().timer(Duration::from_secs(options.hold_seconds.min(600))).await;
                cx.update(|cx| cx.quit());
            }).detach();
        });
}

fn frame_summary(samples: &mut [f64]) -> serde_json::Value {
    samples.sort_by(f64::total_cmp);
    serde_json::json!({ "samples": samples.len(), "median_ms": samples[samples.len()/2],
        "p95_ms": samples[samples.len()*95/100], "max_ms": samples[samples.len()-1] })
}
