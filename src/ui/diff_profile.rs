//! Opt-in native diff timing, with Git IO measured separately from drawing.
use super::*;
use gpui::{
    Bounds, PlatformInput, QuitMode, ScrollDelta, ScrollWheelEvent, TitlebarOptions, WindowBounds,
    WindowOptions, point, size,
};
use gpui_component::Root;
use serde::Deserialize;
use std::time::{Duration, Instant};

#[derive(Deserialize)]
struct Options {
    cwd: PathBuf,
    output: PathBuf,
    #[serde(default)]
    unified: bool,
    #[serde(default)]
    hold_seconds: u64,
}

struct Fixture {
    owner: gpui::Entity<Tty7App>,
    snapshot: Option<DiffSnapshot>,
    scroll: gpui::ScrollHandle,
}

impl gpui::Render for Fixture {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = v_flex().size_full().bg(cx.theme().background);
        if let Some(snap) = &self.snapshot {
            root = root.child(self.owner.update(cx, |app, cx| {
                app.diff_file_list(snap, &HashMap::new(), None, &self.scroll, window, cx)
            }));
        }
        root
    }
}

pub(crate) fn run(path: &std::ffi::OsStr) {
    let host = tty7_core::host::local::LocalHost::new();
    let options: Options =
        serde_json::from_slice(&host.read_file(Path::new(path), 64 * 1024).unwrap()).unwrap();
    let start = Instant::now();
    let snapshot = git_diff::probe(host.as_ref(), &options.cwd).expect("fixture is a repository");
    assert!(!snapshot.read_failed);
    let probe_ms = start.elapsed().as_secs_f64() * 1000.;
    let files = snapshot.files.len();
    let lines = snapshot.stats().retained_lines;
    gpui_platform::application().with_assets(crate::ui::assets::Assets)
        .with_quit_mode(QuitMode::Explicit).run(move |cx| {
            gpui_component::init(cx);
            crate::register_bundled_fonts(cx);
            let mut config = Config::default();
            config.theme_follow_system = false;
            config.theme_preset = "dark".into();
            config.install_cli_on_path = false;
            config.cursor_blink = false;
            config.diff_view = if options.unified { DiffViewMode::Unified } else { DiffViewMode::Split };
            cx.set_global(config);
            crate::core::session::WorkspaceStore::init(cx);
            crate::ui::windows::WindowRegistry::init(cx);
            crate::ui::presets::load_registry(cx);
            crate::ui::keymap::init(cx);
            let mut fixture = None;
            let handle = cx.open_window(WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None,
                    size(px(1000.), px(700.)), cx))),
                titlebar: Some(TitlebarOptions { title: Some("Diff performance fixture".into()), ..Default::default() }),
                show: true, focus: true, ..Default::default()
            }, |window, cx| {
                crate::ui::theme::apply_theme(Some(window), cx);
                let owner = cx.new(|cx| Tty7App::with_session(None,
                    Some(crate::core::session::Session::default()), window, cx));
                let view = cx.new(|_| Fixture { owner, snapshot: None, scroll: gpui::ScrollHandle::new() });
                fixture = Some(view.clone());
                cx.new(|cx| Root::new(view, window, cx))
            }).unwrap();
            let fixture = fixture.unwrap();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                let first_frame_ms = cx.update_window(handle.into(), |_, window, cx| {
                    let start = Instant::now();
                    fixture.update(cx, |view, cx| { view.snapshot = Some(snapshot); cx.notify(); });
                    window.refresh();
                    let _ = window.draw(cx);
                    start.elapsed().as_secs_f64() * 1000.
                }).unwrap();
                let mut samples = Vec::new();
                let mut min_offset = 0_f32;
                for frame in 0..40 {
                    cx.update_window(handle.into(), |_, window, cx| {
                        let start = Instant::now();
                        window.dispatch_event(PlatformInput::ScrollWheel(ScrollWheelEvent {
                            position: point(px(900.), px(350.)),
                            delta: ScrollDelta::Pixels(point(px(0.), px(if frame < 20 { -120. } else { 120. }))),
                            ..Default::default()
                        }), cx);
                        let _ = window.draw(cx);
                        if frame >= 5 { samples.push(start.elapsed().as_secs_f64() * 1000.); }
                        min_offset = min_offset.min(fixture.read(cx).scroll.offset().y.as_f32());
                    }).unwrap();
                }
                assert!(min_offset < -1000., "profile must scroll the production diff");
                samples.sort_by(f64::total_cmp);
                let result = serde_json::json!({
                    "mode": if options.unified { "Unified" } else { "Split" },
                    "files": files, "retained_lines": lines, "git_probe_ms": probe_ms,
                    "first_frame_ms": first_frame_ms, "scroll_samples": samples.len(),
                    "scroll_median_ms": samples[samples.len()/2],
                    "scroll_p95_ms": samples[samples.len()*95/100],
                    "min_offset": min_offset,
                    "measurement": "native text, event dispatch and GPUI CPU layout/prepaint/paint; excludes GPU and presentation",
                });
                let output = options.output.clone();
                let bytes = serde_json::to_vec_pretty(&result).unwrap();
                cx.background_executor().spawn(async move {
                    host.write_file(&output, &bytes).unwrap();
                }).await;
                eprintln!("NATIVE_DIFF_PROFILE {result}");
                cx.background_executor().timer(Duration::from_secs(options.hold_seconds.min(600))).await;
                cx.update_window(handle.into(), |_, window, _| window.remove_window()).unwrap();
                drop(fixture);
                cx.update(|cx| cx.quit());
            }).detach();
        });
}
