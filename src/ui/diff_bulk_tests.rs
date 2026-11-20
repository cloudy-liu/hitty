use super::test_support::{draw, file};
use super::*;
use crate::ui::app::test_window;
use gpui::{Entity, TestAppContext, VisualTestContext, point, size};

struct DiffDocument(Entity<HittyApp>);

impl Render for DiffDocument {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.update(cx, |app, cx| {
            v_flex().size_full().child(
                app.render_diff_overlay(DocumentChrome::Fill, window, cx)
                    .unwrap_or_else(|| div().into_any_element()),
            )
        })
    }
}

fn install(
    app: &Entity<HittyApp>,
    vcx: &mut VisualTestContext,
    files: Vec<FileDiff>,
    expanded: HashMap<String, bool>,
) {
    app.update_in(vcx, |app, _, cx| {
        app.tabs = vec![crate::ui::app::Tab::new(crate::ui::pane::Pane::Empty)];
        app.active = 0;
        app.tabs[0].diff_overlay = Some(DiffOverlayState {
            pane_host: None,
            host_id: crate::ui::host_ops::HostId::LOCAL,
            cwd: PathBuf::from("/fixture"),
            source: DiffSource::Worktree,
            commit: None,
            commit_message_expanded: false,
            commit_split: CommitSplit::new(cx),
            document_header_focus: cx.focus_handle(),
            focus_handle: cx.focus_handle(),
            load: DiffLoad::Ready(Arc::new(DiffSnapshot {
                files,
                source: DiffSource::Worktree,
                ..Default::default()
            })),
            loading: false,
            expanded,
            focus: None,
            preview: None,
            preview_loading: None,
            scroll: gpui::ScrollHandle::new(),
            epoch: None,
            file_views: HashMap::new(),
            geometry: Rc::default(),
            anchor: Rc::default(),
            file_scroll: Rc::default(),
        });
        cx.notify();
    });
    vcx.run_until_parked();
}

fn assert_file_open(vcx: &mut VisualTestContext, index: usize, open: bool) {
    let card = vcx
        .debug_bounds(Box::leak(format!("diff-card-{index}").into_boxed_str()))
        .unwrap();
    let header = vcx
        .debug_bounds(Box::leak(
            format!("diff-file-header:{index}").into_boxed_str(),
        ))
        .unwrap();
    assert_eq!(
        card.size.height > header.size.height + px(3.),
        open,
        "file {index} body visibility"
    );
}

fn toggle(vcx: &mut VisualTestContext) {
    let button = vcx
        .debug_bounds("diff-bulk-fold-toggle")
        .expect("bulk fold button beside the layout control");
    vcx.simulate_click(button.center(), gpui::Modifiers::none());
    vcx.run_until_parked();
}

#[gpui::test]
fn bulk_fold_collapses_a_mixed_list_then_expands_every_file(cx: &mut TestAppContext) {
    let (app, mut initial_window) = test_window::harness(cx);
    initial_window.update(|window, _| window.remove_window());
    let (_, vcx) = cx.add_window_view(|_, _| DiffDocument(app.clone()));
    vcx.simulate_resize(size(px(1000.), px(600.)));
    install(
        &app,
        vcx,
        vec![
            file("first.rs", 2),
            file("second.rs", 2),
            file("third.rs", 2),
        ],
        HashMap::from([("second.rs".into(), false)]),
    );
    assert_file_open(vcx, 0, true);
    assert_file_open(vcx, 1, false);
    assert_file_open(vcx, 2, true);

    toggle(vcx);
    for index in 0..3 {
        assert_file_open(vcx, index, false);
    }
    assert!(vcx.debug_bounds("diff-new-text").is_none());

    toggle(vcx);
    for index in 0..3 {
        assert_file_open(vcx, index, true);
    }
    assert!(vcx.debug_bounds("diff-new-text").is_some());
}

#[gpui::test]
fn bulk_fold_keeps_the_file_being_read_visible_in_both_layouts(cx: &mut TestAppContext) {
    let (app, mut initial_window) = test_window::harness(cx);
    initial_window.update(|window, _| window.remove_window());
    for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
        let (_, vcx) = cx.add_window_view(|_, _| DiffDocument(app.clone()));
        vcx.simulate_resize(size(px(1000.), px(600.)));
        vcx.update(|_, cx| cx.global_mut::<Config>().diff_view = mode);
        install(
            &app,
            vcx,
            (0..24)
                .map(|index| file(&format!("file-{index}.rs"), 15))
                .collect(),
            HashMap::new(),
        );
        let viewport = vcx.debug_bounds("diff-scroll").unwrap();
        let reading = vcx.debug_bounds("diff-card-9").unwrap();
        vcx.simulate_event(gpui::ScrollWheelEvent {
            position: viewport.center(),
            delta: gpui::ScrollDelta::Pixels(point(
                px(0.),
                viewport.top() - reading.top() - px(120.),
            )),
            ..Default::default()
        });
        draw(vcx);
        let reading = vcx.debug_bounds("diff-card-9").unwrap();
        assert!(reading.top() < viewport.top() && reading.bottom() > viewport.top());

        toggle(vcx);
        draw(vcx);
        let header = vcx.debug_bounds("diff-file-header:9").unwrap();
        assert!(
            header.top() >= viewport.top() && header.bottom() <= viewport.bottom(),
            "the file being read must stay visible after collapse in {mode:?}: {header:?} / {viewport:?}"
        );

        toggle(vcx);
        draw(vcx);
        let reading = vcx.debug_bounds("diff-card-9").unwrap();
        assert!(
            reading.top() < viewport.bottom() && reading.bottom() > viewport.top(),
            "expanding preceding files must keep the reader's file visible in {mode:?}"
        );
        vcx.update(|window, _| window.remove_window());
    }
}

#[gpui::test]
fn bulk_fold_keeps_the_current_reader_after_switching_layout(cx: &mut TestAppContext) {
    let (app, mut initial_window) = test_window::harness(cx);
    initial_window.update(|window, _| window.remove_window());
    for source in [
        DiffSource::Worktree,
        DiffSource::Commit {
            rev: "0123456789abcdef".into(),
            label: Some(CommitLabel {
                subject: "Replace lines across files".into(),
                author: "Ada".into(),
                at: 0,
            }),
        },
    ] {
        let (_, vcx) = cx.add_window_view(|_, _| DiffDocument(app.clone()));
        vcx.simulate_resize(size(px(1000.), px(700.)));
        vcx.update(|_, cx| cx.global_mut::<Config>().diff_view = DiffViewMode::Split);
        let patch = (0..60)
            .map(|index| {
                let path = format!("file-{index}.rs");
                let removed = (0..15)
                    .map(|line| format!("-old {line}\n"))
                    .collect::<String>();
                let added = (0..15)
                    .map(|line| format!("+new {line}\n"))
                    .collect::<String>();
                format!(
                    "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,15 +1,15 @@\n{removed}{added}"
                )
            })
            .collect::<String>();
        let mut parser = git_diff::DiffParser::default();
        for line in patch.lines() {
            parser.push_line(line);
        }
        install(&app, vcx, parser.finish(), HashMap::new());
        app.update(vcx, |app, cx| {
            app.tabs[0].diff_overlay.as_mut().unwrap().source = source.clone();
            cx.notify();
        });
        draw(vcx);
        assert_eq!(
            vcx.debug_bounds("commit-diff-toolbar").is_some(),
            matches!(source, DiffSource::Commit { .. }),
            "exercise the controls for {source:?}"
        );
        let viewport = vcx.debug_bounds("diff-scroll").unwrap();
        let reading = vcx.debug_bounds("diff-card-45").unwrap();
        vcx.simulate_event(gpui::ScrollWheelEvent {
            position: viewport.center(),
            delta: gpui::ScrollDelta::Pixels(point(
                px(0.),
                viewport.top() - reading.top() - px(120.),
            )),
            ..Default::default()
        });
        draw(vcx);
        toggle(vcx);
        draw(vcx);
        toggle(vcx);
        draw(vcx);
        let reading = vcx.debug_bounds("diff-card-45").unwrap();
        assert!(reading.top() < viewport.bottom() && reading.bottom() > viewport.top());

        let layout = vcx.debug_bounds("diff-view-toggle").unwrap();
        vcx.simulate_click(layout.center(), gpui::Modifiers::none());
        draw(vcx);
        assert!(vcx.debug_bounds("diff-unified-text").is_some());
        let current = (0..60)
            .find(|index| {
                let card = vcx
                    .debug_bounds(Box::leak(format!("diff-card-{index}").into_boxed_str()))
                    .unwrap();
                card.bottom() > viewport.top() && card.top() < viewport.bottom()
            })
            .expect("the unified diff has a visible file");
        assert_ne!(
            current, 45,
            "replacement hunks must change the visible file"
        );
        assert!(current > 20, "the reader remains in the middle of the list");

        toggle(vcx);
        draw(vcx);
        let header = vcx
            .debug_bounds(Box::leak(
                format!("diff-file-header:{current}").into_boxed_str(),
            ))
            .unwrap();
        assert!(
            header.top() >= viewport.top() && header.bottom() <= viewport.bottom(),
            "collapsing {source:?} after a layout switch must keep current file {current} visible: {header:?} / {viewport:?}"
        );
        vcx.update(|window, _| window.remove_window());
    }
}

#[gpui::test]
fn bulk_fold_remembers_the_reader_when_collapsed_files_fit_the_viewport(cx: &mut TestAppContext) {
    let (app, mut initial_window) = test_window::harness(cx);
    initial_window.update(|window, _| window.remove_window());
    let (_, vcx) = cx.add_window_view(|_, _| DiffDocument(app.clone()));
    vcx.simulate_resize(size(px(1000.), px(600.)));
    install(
        &app,
        vcx,
        (0..8)
            .map(|index| file(&format!("file-{index}.rs"), 15))
            .collect(),
        HashMap::new(),
    );
    let viewport = vcx.debug_bounds("diff-scroll").unwrap();
    let reading = vcx.debug_bounds("diff-card-5").unwrap();
    vcx.simulate_event(gpui::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), viewport.top() - reading.top() - px(120.))),
        ..Default::default()
    });
    draw(vcx);
    toggle(vcx);
    draw(vcx);
    let first = vcx.debug_bounds("diff-file-header:0").unwrap();
    let last = vcx.debug_bounds("diff-file-header:7").unwrap();
    assert!(first.top() >= viewport.top() && last.bottom() <= viewport.bottom());

    toggle(vcx);
    draw(vcx);
    let reading = vcx.debug_bounds("diff-card-5").unwrap();
    assert!(
        reading.top() < viewport.bottom() && reading.bottom() > viewport.top(),
        "the reader's file survives an expand/collapse round trip even when collapse clamps to zero"
    );
}

#[gpui::test]
fn bulk_fold_explicitly_opens_large_files_without_changing_the_initial_default(
    cx: &mut TestAppContext,
) {
    let (app, mut initial_window) = test_window::harness(cx);
    initial_window.update(|window, _| window.remove_window());
    let (_, vcx) = cx.add_window_view(|_, _| DiffDocument(app.clone()));
    vcx.simulate_resize(size(px(1000.), px(600.)));
    install(
        &app,
        vcx,
        vec![file("small.rs", 2), file("large.rs", 501)],
        HashMap::new(),
    );
    assert_file_open(vcx, 0, true);
    assert_file_open(vcx, 1, false);
    toggle(vcx);
    assert_file_open(vcx, 0, false);
    assert_file_open(vcx, 1, false);
    toggle(vcx);
    assert_file_open(vcx, 0, true);
    assert_file_open(vcx, 1, true);
}

#[gpui::test]
fn bulk_fold_expands_oversized_and_truncated_cards_within_the_existing_file_limit(
    cx: &mut TestAppContext,
) {
    let (app, mut initial_window) = test_window::harness(cx);
    initial_window.update(|window, _| window.remove_window());
    let (_, vcx) = cx.add_window_view(|_, _| DiffDocument(app.clone()));
    vcx.simulate_resize(size(px(1000.), px(600.)));
    let mut files: Vec<_> = (0..=MAX_RENDERED_FILES)
        .map(|index| file(&format!("file-{index}.rs"), 1))
        .collect();
    files[0].hunks.clear();
    files[0].truncated = Some(Truncation::Budget);
    files[1].truncated = Some(Truncation::PerFile);
    install(&app, vcx, files, HashMap::new());
    assert_file_open(vcx, 0, false);
    assert_file_open(vcx, 1, false);
    assert_file_open(vcx, MAX_RENDERED_FILES - 1, false);
    assert!(vcx.debug_bounds("diff-card-300").is_none());
    toggle(vcx);
    draw(vcx);
    assert_file_open(vcx, 0, true);
    assert_file_open(vcx, 1, true);
    assert_file_open(vcx, MAX_RENDERED_FILES - 1, true);
    assert!(vcx.debug_bounds("diff-card-300").is_none());
}

#[gpui::test]
fn bulk_fold_is_hidden_without_eligible_cards_and_in_single_file_focus(cx: &mut TestAppContext) {
    let (app, mut initial_window) = test_window::harness(cx);
    initial_window.update(|window, _| window.remove_window());
    let (_, vcx) = cx.add_window_view(|_, _| DiffDocument(app.clone()));
    vcx.simulate_resize(size(px(1000.), px(600.)));
    let mut binary = file("image.png", 0);
    binary.binary = true;
    for files in [vec![], vec![binary, file("metadata.rs", 0)]] {
        install(&app, vcx, files, HashMap::new());
        assert!(vcx.debug_bounds("diff-bulk-fold-toggle").is_none());
    }
    for load in [DiffLoad::Loading, DiffLoad::Unavailable, DiffLoad::NotARepo] {
        app.update(vcx, |app, cx| {
            app.tabs[0].diff_overlay.as_mut().unwrap().load = load;
            cx.notify();
        });
        draw(vcx);
        assert!(vcx.debug_bounds("diff-bulk-fold-toggle").is_none());
    }

    install(&app, vcx, vec![file("focused.rs", 2)], HashMap::new());
    app.update(vcx, |app, cx| {
        app.tabs[0].diff_overlay.as_mut().unwrap().focus = Some("focused.rs".into());
        cx.notify();
    });
    draw(vcx);
    assert!(vcx.debug_bounds("diff-bulk-fold-toggle").is_none());
    assert!(vcx.debug_bounds("diff-view-toggle").is_some());
    assert!(vcx.debug_bounds("diff-context-button:focused.rs").is_some());
    assert_file_open(vcx, 0, true);
}

#[gpui::test]
fn bulk_fold_can_be_reached_and_activated_from_the_keyboard(cx: &mut TestAppContext) {
    let (app, mut initial_window) = test_window::harness(cx);
    initial_window.update(|window, _| window.remove_window());
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|_| DiffDocument(app.clone()));
        gpui_component::Root::new(view, window, cx)
    });
    vcx.simulate_resize(size(px(1000.), px(600.)));
    install(&app, vcx, vec![file("keyboard.rs", 2)], HashMap::new());
    app.update_in(vcx, |app, window, cx| {
        window.focus(&app.tabs[0].diff_overlay.as_ref().unwrap().focus_handle, cx);
    });
    vcx.simulate_keystrokes("tab");
    draw(vcx);
    vcx.simulate_keystrokes("enter");
    vcx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
    });
    draw(vcx);
    assert_file_open(vcx, 0, false);
    vcx.simulate_keystrokes("space");
    vcx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("space").unwrap(),
    });
    draw(vcx);
    assert_file_open(vcx, 0, true);
}
