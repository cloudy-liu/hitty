//! Opt-in native performance fixture. Clicks the actual sidebar button and
//! dispatches the production split action, with real daemon panes and profiles.
use gpui::{prelude::*, *};
use gpui_component::Root;
use serde::Deserialize;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use crate::{core::config::Config, terminal::view::TerminalView, ui::app::HittyApp};

thread_local! {
    static BUTTONS: RefCell<BTreeMap<PathBuf, Bounds<Pixels>>> = RefCell::new(BTreeMap::new());
}
static ENABLED: AtomicBool = AtomicBool::new(false);

// Only compiled into the existing native-fixture build. The canvas observes
// layout without adding a hitbox or replacing the real button's listener.
pub(crate) fn observe_button(button: AnyElement, path: Option<PathBuf>) -> AnyElement {
    if !ENABLED.load(Ordering::Relaxed) {
        return button;
    }
    div()
        .relative()
        .child(button)
        .child(
            canvas(
                move |bounds, _, _| {
                    if let Some(path) = &path {
                        BUTTONS.with(|buttons| buttons.borrow_mut().insert(path.clone(), bounds));
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .into_any_element()
}

#[derive(Deserialize)]
struct Options {
    cwd: PathBuf,
    output: PathBuf,
    samples: usize,
    budget_ms: Option<f64>,
    hold_seconds: Option<u64>,
}

fn millis(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.
}

fn views(app: &HittyApp) -> Vec<Entity<TerminalView>> {
    app.tabs
        .iter()
        .flat_map(|tab| tab.pane.terminals())
        .collect()
}

fn screen(view: &TerminalView) -> String {
    use alacritty_terminal::{
        grid::Dimensions,
        index::{Column, Line},
    };
    let term = view.terminal.term.lock();
    let grid = term.grid();
    (0..grid.screen_lines())
        .map(|row| {
            (0..grid.columns())
                .map(|col| grid[Line(row as i32)][Column(col)].c)
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn wait_prompt(
    app: &Entity<HittyApp>,
    old: &[EntityId],
    cx: &mut AsyncApp,
) -> anyhow::Result<Entity<TerminalView>> {
    let started = Instant::now();
    loop {
        if let Some(view) = app.read_with(cx, |app, cx| {
            views(app)
                .into_iter()
                .find(|view| !old.contains(&view.entity_id()) && view.read(cx).terminal.at_prompt())
        }) {
            return Ok(view);
        }
        anyhow::ensure!(
            started.elapsed() < Duration::from_secs(30),
            "new pane did not reach a prompt"
        );
        cx.background_executor()
            .timer(Duration::from_millis(5))
            .await;
    }
}

async fn command(
    view: &Entity<TerminalView>,
    input: &'static [u8],
    expected: Option<&str>,
    cx: &mut AsyncApp,
) -> anyhow::Result<f64> {
    let (seq, previous_matches) = view.read_with(cx, |view, _| {
        let seq = view.terminal.prompt_seq();
        let previous_matches = expected.map_or(0, |text| screen(view).matches(text).count());
        view.terminal.write(input);
        (seq, previous_matches)
    });
    let started = Instant::now();
    loop {
        if view.read_with(cx, |view, _| {
            view.terminal.at_prompt()
                && view.terminal.prompt_seq() > seq
                && expected.is_none_or(|expected| {
                    screen(view).matches(expected).count() > previous_matches
                })
        }) {
            return Ok(millis(started));
        }
        anyhow::ensure!(
            started.elapsed() < Duration::from_secs(10),
            "input/prompt validation timed out: {:?}",
            input
        );
        cx.background_executor()
            .timer(Duration::from_millis(5))
            .await;
    }
}

pub(crate) fn run(path: &std::ffi::OsStr) {
    let options: Options = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!((1..=100).contains(&options.samples));
    assert!(options.budget_ms.is_none_or(|v| v.is_finite() && v > 0.));
    let profile =
        crate::core::config::config_dir_path().expect("explicit isolated config required");
    assert!(
        profile.join("terminal-startup-fixture").is_file(),
        "fixture ownership marker required"
    );
    ENABLED.store(true, Ordering::Relaxed);
    let started = Instant::now();
    crate::daemon::spawn::ensure_running().unwrap();
    let daemon_ms = millis(started);
    gpui_platform::application().with_assets(crate::ui::assets::Assets)
        .with_quit_mode(QuitMode::Explicit).run(move |cx| {
            gpui_component::init(cx);
            crate::register_bundled_fonts(cx);
            cx.set_global(Config::load());
            crate::core::session::WorkspaceStore::init(cx);
            crate::ui::windows::WindowRegistry::init(cx);
            crate::ui::presets::load_registry(cx);
            crate::ui::keymap::init(cx);
            let mut owner = None;
            let handle = cx.open_window(WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(40.), px(40.)), size(px(1100.), px(700.))))),
                titlebar: Some(TitlebarOptions { title: Some("Terminal startup measurement".into()), ..gpui_component::TitleBar::title_bar_options() }),
                show: true, focus: true, ..Default::default()
            }, |window, cx| {
                crate::ui::theme::apply_theme(Some(window), cx);
                let app = cx.new(|cx| HittyApp::for_workspace_at(None, Some(options.cwd.clone()), window, cx));
                owner = Some(app.clone());
                cx.new(|cx| Root::new(app, window, cx))
            }).unwrap();
            cx.activate(true);
            let app = owner.unwrap();
            crate::ui::windows::WindowRegistry::register(cx, app.read(cx).workspace, handle.into(), app.downgrade());
            cx.spawn(async move |cx| {
                let mut results = Vec::new();
                let mut retired_tabs = Vec::new();
                let outcome: anyhow::Result<()> = async {
                    let first = wait_prompt(&app, &[], cx).await?;
                    results.push(serde_json::json!({"entry":"cold_fixture", "daemon_ms":daemon_ms, "prompt_ms":millis(started)}));
                    // Let sidebar repository detection and the first frame settle
                    // outside the additional-pane timing interval.
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    command(&first, b"hitty-startup-fixtu\t\r", Some("73421"), cx).await?;
                    for sample in 0..options.samples {
                        for entry in ["sidebar_button", "split_action"] {
                            let old = app.read_with(cx, |app, _| views(app).iter().map(Entity::entity_id).collect::<Vec<_>>());
                            let started = cx.update_window(handle.into(), |_, window, cx| -> anyhow::Result<Instant> {
                                if entry == "sidebar_button" {
                                    window.refresh(); let _ = window.draw(cx);
                                    let bounds = BUTTONS.with(|buttons| buttons.borrow().get(&options.cwd).copied())
                                        .ok_or_else(|| anyhow::anyhow!("project sidebar button was not laid out"))?;
                                    window.simulate_mouse_move(bounds.center(), cx);
                                    window.refresh(); let _ = window.draw(cx);
                                    let started = Instant::now();
                                    window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent {
                                        position: bounds.center(), button: MouseButton::Left, click_count: 1, ..Default::default()
                                    }), cx);
                                    window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent {
                                        position: bounds.center(), button: MouseButton::Left, click_count: 1, ..Default::default()
                                    }), cx);
                                    Ok(started)
                                } else {
                                    app.update(cx, |app, cx| app.focus_active(window, cx));
                                    let started = Instant::now();
                                    window.dispatch_action(Box::new(crate::core::actions::SplitRight), cx);
                                    Ok(started)
                                }
                            })??;
                            let view = wait_prompt(&app, &old, cx).await?;
                            let prompt_ms = millis(started);
                            cx.update_window(handle.into(), |_, window, cx| { window.refresh(); let _ = window.draw(cx); })?;
                            let draw_after_prompt_ms = millis(started);
                            anyhow::ensure!(app.read_with(cx, |app, _| views(app).len()) == old.len() + 1,
                                "{entry} did not add exactly one terminal");
                            let command_ms = command(&view, b"hitty-startup-fixtu\t\r", Some("73421"), cx).await?;
                            command(&view, b"\x1b[A\r", Some("73421"), cx).await?;
                            command(&view, b"echo unsubmitted\x03", None, cx).await?;
                            anyhow::ensure!(!view.read_with(cx, |view, _| screen(view).lines().any(|line| line.trim() == "unsubmitted")),
                                "Ctrl+C executed the unfinished input");
                            command(&view, b"\r", None, cx).await?;
                            command(&view, b"hitty-startup-fixture\r", Some("73421"), cx).await?;
                            results.push(serde_json::json!({"entry":entry,"sample":sample,"prompt_ms":prompt_ms,
                                "draw_after_prompt_ms":draw_after_prompt_ms,"first_command_ms":command_ms,
                                "completion_history_ctrl_c_prompt_refresh":"passed"}));
                            eprintln!("{}", results.last().unwrap());
                            std::fs::write(&options.output, serde_json::to_vec_pretty(&results)?)?;
                        }
                        // Reset fixture state outside the timed interval. Keep
                        // entities alive until quit rather than invoke the
                        // interactive close/worktree-confirmation workflow.
                        let retired = cx.update_window(handle.into(), |_, window, cx| app.update(cx, |app, cx| {
                            let retired = app.tabs.pop().expect("measured tab");
                            app.active = 0;
                            app.focus_active(window, cx);
                            cx.notify();
                            retired
                        }))?;
                        let panes = retired.pane.terminals().iter().map(|view| view.read_with(cx, |view, _|
                            (view.pane_route(), view.pane_id))).collect::<Vec<_>>();
                        cx.background_executor().spawn(async move {
                            for (route, pane_id) in panes {
                                crate::terminal::RemoteTerminal::kill_pane_on(&route, pane_id);
                            }
                        }).await;
                        retired_tabs.push(retired);
                        cx.background_executor().timer(Duration::from_millis(100)).await;
                    }
                    if let Some(seconds) = options.hold_seconds {
                        cx.background_executor().timer(Duration::from_secs(seconds)).await;
                    }
                    if let Some(budget) = options.budget_ms {
                        for entry in ["sidebar_button", "split_action"] {
                            let mut times: Vec<_> = results.iter().filter(|r| r["entry"] == entry)
                                .map(|r| r["draw_after_prompt_ms"].as_f64().unwrap()).collect();
                            times.sort_by(f64::total_cmp);
                            let p95 = times[(times.len()*95).div_ceil(100)-1];
                            anyhow::ensure!(p95 < budget, "{entry} P95 {p95:.2}ms exceeds {budget:.2}ms");
                        }
                    }
                    Ok(())
                }.await;
                if let Err(error) = &outcome { results.push(serde_json::json!({"error":format!("{error:#}")})); }
                std::fs::write(&options.output, serde_json::to_vec_pretty(&results).unwrap()).unwrap();
                eprintln!("TERMINAL_STARTUP_{}", if outcome.is_ok() { "PASSED" } else { "FAILED" });
                cx.update(|cx| cx.quit());
            }).detach();
        });
}
