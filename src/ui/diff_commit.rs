//! Commit details, split resizing, and the fixed toolbar above the diff.
use super::*;
use gpui::{Bounds, MouseButton, MouseMoveEvent, MouseUpEvent};
use gpui_component::InteractiveElementExt as _;
use hitty_core::core::date::format_date;
use std::cell::Cell;

impl HittyApp {
    pub(crate) fn has_commit_document(&self) -> bool {
        self.tabs
            .get(self.active)
            .and_then(|tab| tab.diff_overlay.as_ref())
            .is_some_and(|overlay| {
                matches!(&overlay.source, DiffSource::Commit { label: Some(_), .. })
            })
    }

    pub(super) fn commit_document_breadcrumb(
        &self,
        overlay: &DiffOverlayState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let root = overlay
            .cwd
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        h_flex()
            .id("commit-document-breadcrumb")
            .debug_selector(|| "commit-document-breadcrumb".into())
            .flex_none()
            .h(px(crate::ui::app::SECONDARY_BAR_HEIGHT))
            .px_4()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                Icon::new(IconName::Folder)
                    .small()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(div().flex_1().min_w_0().truncate().text_sm().child(format!(
                "{root}  \u{203a}  {}",
                t(L10nKey::ScmCommitChanges)
            )))
            .when_some(focused_name(overlay), |bar, name| {
                bar.child(
                    h_flex()
                        .id("commit-document-unfocus")
                        .debug_selector(|| "commit-document-unfocus".into())
                        .max_w(px(180.))
                        .min_w_0()
                        .gap_1()
                        .px_1()
                        .items_center()
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(overlay) = this
                                .tabs
                                .get_mut(this.active)
                                .and_then(|tab| tab.diff_overlay.as_mut())
                            {
                                overlay.focus = None;
                                *overlay.anchor.borrow_mut() = None;
                                *overlay.file_scroll.borrow_mut() = DiffFileScroll::default();
                                cx.notify();
                            }
                        }))
                        .child(Icon::new(IconName::ChevronLeft).small())
                        .child(div().min_w_0().truncate().text_xs().child(name)),
                )
            })
            .into_any_element()
    }

    pub(super) fn diff_commit_details(
        &self,
        overlay: &DiffOverlayState,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let DiffSource::Commit {
            rev,
            label: Some(label),
        } = &overlay.source
        else {
            return None;
        };
        let commit = overlay.commit.as_deref();
        let image = cx
            .global::<Config>()
            .scm_remote_avatars
            .then(|| self.scm.graph.avatars.as_ref())
            .flatten()
            .zip(commit)
            .and_then(|(cache, commit)| {
                cache.update(cx, |cache, cx| cache.image(&commit.author.email, cx))
            });
        let avatar = crate::ui::scm::avatars::author_avatar(label.author.clone(), image, 36.);
        let date = format_date(label.at, commit.map_or(0, |c| c.author.at.offset_minutes));
        let mono = cx.theme().mono_font_family.clone();
        let muted = cx.theme().muted_foreground;
        let full_sha = rev.clone();
        let mut details = v_flex()
            .flex_none()
            .w_full()
            .gap_3()
            .text_color(cx.theme().foreground)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_color(muted)
                    .child(gpui::svg().path("icons/git-commit.svg").size(px(14.)))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .child(t(L10nKey::ScmViewCommitDetails).to_uppercase()),
                    ),
            )
            .child(
                div()
                    .id("diff-commit-title")
                    .debug_selector(|| "diff-commit-title".into())
                    .text_size(gpui::rems(1.5))
                    .line_height(gpui::rems(1.875))
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_clamp(3)
                    .child(label.subject.clone())
                    .tooltip({
                        let title = label.subject.clone();
                        move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(title.clone()).build(window, cx)
                        }
                    }),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .child(avatar)
                    .child(
                        div()
                            .debug_selector(|| "diff-commit-author".into())
                            .min_w_0()
                            .flex_shrink(1.)
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(label.author.clone()),
                    )
                    .child(
                        div()
                            .id("diff-commit-date")
                            .text_xs()
                            .text_color(muted)
                            .child(date)
                            .tooltip({
                                let text = label_byline(label, now_unix());
                                move |window, cx| {
                                    gpui_component::tooltip::Tooltip::new(text.clone())
                                        .build(window, cx)
                                }
                            }),
                    )
                    .child(div().flex_1())
                    .child(
                        div().debug_selector(|| "diff-commit-copy".into()).child(
                            Button::new("diff-commit-copy")
                                .small()
                                .ghost()
                                .icon(IconName::Copy)
                                .label(rev.chars().take(8).collect::<String>())
                                .tooltip(t(L10nKey::ScmCopyCommitSha))
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        full_sha.clone(),
                                    ))
                                }),
                        ),
                    ),
            )
            .when_some(
                commit.filter(|commit| !commit.refs.is_empty()),
                |details, commit| {
                    let refs = h_flex()
                        .flex_wrap()
                        .gap_2()
                        .children(commit.refs.iter().take(8).enumerate().map(
                            |(index, reference)| {
                                let (background, foreground) =
                                    if reference.kind == hitty_core::core::git::log::RefKind::Tag {
                                        (cx.theme().warning.opacity(0.16), cx.theme().warning)
                                    } else {
                                        (cx.theme().accent.opacity(0.16), muted)
                                    };
                                let full = reference.full.clone();
                                div()
                                    .id(("commit-reference", index))
                                    .tooltip(move |window, cx| {
                                        gpui_component::tooltip::Tooltip::new(full.clone())
                                            .build(window, cx)
                                    })
                                    .child(info_chip(
                                        &crate::ui::scm::path::elide_middle(&reference.short, 24),
                                        background,
                                        foreground,
                                        &mono,
                                    ))
                            },
                        ))
                        .when_some(
                            overlay
                                .commit
                                .clone()
                                .filter(|commit| commit.refs.len() > 8),
                            |refs, commit| {
                                refs.child(
                                    div()
                                        .id("commit-ref-overflow")
                                        .text_xs()
                                        .text_color(muted)
                                        .child(format!("+{}", commit.refs.len() - 8))
                                        .tooltip(move |window, cx| {
                                            let names = commit
                                                .refs
                                                .iter()
                                                .skip(8)
                                                .map(|reference| reference.full.as_str())
                                                .collect::<Vec<_>>()
                                                .join("\n");
                                            gpui_component::tooltip::Tooltip::new(names)
                                                .build(window, cx)
                                        }),
                                )
                            },
                        );
                    details.child(refs)
                },
            );
        if let Some(body) = commit
            .map(|commit| commit.body.trim())
            .filter(|body| !body.is_empty())
        {
            let can_fold = body.lines().count() > 4;
            let folded = can_fold && !overlay.commit_message_expanded;
            let visible = if folded {
                body.lines().take(4).collect::<Vec<_>>().join("\n")
            } else {
                body.to_string()
            };
            details = details
                .child(
                    div()
                        .id("diff-commit-body")
                        .debug_selector(|| "diff-commit-body".into())
                        .text_sm()
                        .line_height(px(20.))
                        .text_color(muted)
                        .when(folded, |body| body.max_h(px(80.)).overflow_hidden())
                        .child(visible),
                )
                .when(can_fold, |details| {
                    details.child(
                        div()
                            .debug_selector(|| "diff-commit-body-toggle".into())
                            .child(
                                Button::new("diff-commit-body-toggle")
                                    .small()
                                    .ghost()
                                    .label(t(if folded {
                                        L10nKey::ScmShowMore
                                    } else {
                                        L10nKey::ScmShowLess
                                    }))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(overlay) = this
                                            .tabs
                                            .get_mut(this.active)
                                            .and_then(|tab| tab.diff_overlay.as_mut())
                                        {
                                            overlay.commit_message_expanded =
                                                !overlay.commit_message_expanded;
                                            cx.notify();
                                        }
                                    })),
                            ),
                    )
                });
        }
        Some(
            v_flex()
                .id("diff-commit-details")
                .debug_selector(|| "diff-commit-details".into())
                .flex_shrink_0()
                .h(gpui::relative(overlay.commit_split.ratio()))
                .min_h_0()
                .overflow_y_scroll()
                .px_4()
                .py_4()
                .child(details)
                .into_any_element(),
        )
    }

    pub(super) fn diff_commit_toolbar(
        &self,
        overlay: &DiffOverlayState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let stats = match &overlay.load {
            DiffLoad::Ready(snapshot) => {
                let (added, removed) = snapshot.stats().totals;
                Some(
                    h_flex()
                        .debug_selector(|| "diff-commit-stats".into())
                        .min_w_0()
                        .overflow_hidden()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .child(
                            Icon::new(IconName::Folder)
                                .small()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(cx.theme().muted_foreground)
                                .child(t_plural(
                                    L10nKey::DiffChangedFiles,
                                    snapshot.files.len(),
                                    &[],
                                )),
                        )
                        .child(
                            div()
                                .flex_none()
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_color(cx.theme().success)
                                .child(format!("+{added}")),
                        )
                        .child(
                            div()
                                .flex_none()
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_color(cx.theme().danger)
                                .child(format!("\u{2212}{removed}")),
                        ),
                )
            }
            _ => None,
        };
        h_flex()
            .id("commit-diff-toolbar")
            .debug_selector(|| "commit-diff-toolbar".into())
            .flex_none()
            .h(px(34.))
            .w_full()
            .min_w_0()
            .px_4()
            .gap_3()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().flex_1().min_w_0().children(stats))
            .child(self.diff_document_actions(overlay, cx))
            .into_any_element()
    }
}

fn split_limits(height: f32) -> (f32, f32) {
    let height = height.max(1.);
    ((100. / height).min(0.4), (1. - 140. / height).max(0.4))
}

/// Per-document geometry, using the same window-level drag capture as the side panels.
pub(crate) struct CommitSplit {
    ratio: Rc<Cell<f32>>,
    dragging: Rc<Cell<bool>>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    focus: FocusHandle,
}

impl CommitSplit {
    pub(crate) fn new(cx: &mut gpui::App) -> Self {
        Self {
            ratio: Rc::new(Cell::new(0.45)),
            dragging: Rc::default(),
            bounds: Rc::default(),
            focus: cx.focus_handle(),
        }
    }

    fn limits(&self) -> (f32, f32) {
        let height = self
            .bounds
            .get()
            .map_or(600., |b| b.size.height.as_f32())
            .max(1.);
        split_limits(height)
    }

    fn ratio(&self) -> f32 {
        let (min, max) = self.limits();
        self.ratio.get().clamp(min, max)
    }

    pub(super) fn backing(&self) -> AnyElement {
        let bounds = self.bounds.clone();
        let ratio = self.ratio.clone();
        let dragging = self.dragging.clone();
        gpui::canvas(
            {
                let bounds = bounds.clone();
                move |actual, window, _| {
                    if bounds
                        .replace(Some(actual))
                        .is_none_or(|old| old.size != actual.size)
                    {
                        window.refresh();
                    }
                }
            },
            move |_, _, window, _| {
                window.on_mouse_event({
                    let bounds = bounds.clone();
                    let ratio = ratio.clone();
                    let dragging = dragging.clone();
                    move |event: &MouseMoveEvent, _, window, _| {
                        if !dragging.get() {
                            return;
                        }
                        // A release outside the window must not leave a stuck drag.
                        if !event.pressed_button.is_some_and(|b| b == MouseButton::Left) {
                            dragging.set(false);
                            window.refresh();
                            return;
                        }
                        if let Some(bounds) = bounds.get() {
                            let height = bounds.size.height.as_f32().max(1.);
                            let (min, max) = split_limits(height);
                            ratio.set(
                                ((event.position.y - bounds.top()).as_f32() / height)
                                    .clamp(min, max),
                            );
                            window.refresh();
                        }
                    }
                });
                window.on_mouse_event({
                    let dragging = dragging.clone();
                    move |_: &MouseUpEvent, _, window, _| {
                        if dragging.replace(false) {
                            window.refresh();
                        }
                    }
                });
            },
        )
        .absolute()
        .inset_0()
        .size_full()
        .into_any_element()
    }

    pub(super) fn divider(&self, cx: &mut Context<HittyApp>) -> AnyElement {
        let ratio = self.ratio.clone();
        let dragging = self.dragging.clone();
        let focus = self.focus.clone();
        let bounds = self.bounds.clone();
        div()
            .id("commit-details-resize")
            .debug_selector(|| "commit-details-resize".into())
            .track_focus(&self.focus)
            .occlude()
            .absolute()
            .top(px(-4.))
            .left_0()
            .w_full()
            .h(px(9.))
            .flex()
            .items_center()
            .cursor_row_resize()
            .child(div().h(px(1.)).w_full().bg(cx.theme().border))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                dragging.set(true);
                window.focus(&focus, cx);
                cx.stop_propagation();
            })
            .on_double_click({
                let ratio = ratio.clone();
                let dragging = self.dragging.clone();
                move |_, window, cx| {
                    dragging.set(false);
                    ratio.set(0.45);
                    window.refresh();
                    cx.stop_propagation();
                }
            })
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let height = bounds
                    .get()
                    .map_or(600., |b| b.size.height.as_f32())
                    .max(1.);
                let (min, max) = split_limits(height);
                let delta = if event.keystroke.modifiers.shift {
                    32.
                } else {
                    8.
                } / height;
                let current = ratio.get().clamp(min, max);
                let value = match event.keystroke.key.as_str() {
                    "up" => current - delta,
                    "down" => current + delta,
                    "home" => min,
                    "end" => max,
                    _ => return,
                };
                ratio.set(value.clamp(min, max));
                window.refresh();
                cx.stop_propagation();
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{draw, file};
    use super::*;
    use gpui::{Entity, TestAppContext, VisualTestContext, point, size};

    struct CommitDocument(Entity<HittyApp>);

    impl Render for CommitDocument {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.0.update(cx, |app, cx| {
                v_flex().size_full().children(app.render_diff_overlay(
                    DocumentChrome::Fill,
                    window,
                    cx,
                ))
            })
        }
    }

    fn install(app: &Entity<HittyApp>, vcx: &mut VisualTestContext, files: Vec<FileDiff>) {
        app.update_in(vcx, |app, window, cx| {
            app.tabs = vec![crate::ui::app::Tab::new(crate::ui::pane::Pane::Empty)];
            app.active = 0;
            app.open_diff_overlay(
                crate::ui::host_ops::HostId::from_connection_key("commit-toolbar-fixture"),
                "/fixture".into(),
                DiffSource::Commit {
                    rev: "0123456789abcdef".into(),
                    label: Some(git_diff::CommitLabel {
                        subject: "Review the commit changes".into(),
                        author: "Ada".into(),
                        at: 0,
                    }),
                },
                None,
                window,
                cx,
            );
            let overlay = app.tabs[0].diff_overlay.as_mut().unwrap();
            overlay.loading = false;
            overlay.load = DiffLoad::Ready(Arc::new(DiffSnapshot {
                files,
                source: overlay.source.clone(),
                ..Default::default()
            }));
            cx.notify();
        });
        draw(vcx);
    }

    #[gpui::test]
    fn commit_toolbar_stays_fixed_and_bulk_folding_keeps_the_reader(cx: &mut TestAppContext) {
        let (app, mut initial_window) = crate::ui::app::test_window::harness(cx);
        initial_window.update(|window, _| window.remove_window());
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            let text_selector = match mode {
                DiffViewMode::Split => "diff-new-text",
                DiffViewMode::Unified => "diff-unified-text",
            };
            let (_, vcx) = cx.add_window_view(|_, _| CommitDocument(app.clone()));
            vcx.simulate_resize(size(px(1000.), px(700.)));
            vcx.update(|_, cx| cx.global_mut::<Config>().diff_view = mode);
            install(
                &app,
                vcx,
                (0..24)
                    .map(|index| file(&format!("file-{index}.rs"), 15))
                    .collect(),
            );
            let toolbar = vcx.debug_bounds("commit-diff-toolbar").unwrap();
            let totals = vcx.debug_bounds("diff-commit-stats").unwrap();
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
            assert_eq!(vcx.debug_bounds("commit-diff-toolbar"), Some(toolbar));
            assert_eq!(vcx.debug_bounds("diff-commit-stats"), Some(totals));

            let toggle = vcx
                .debug_bounds("diff-bulk-fold-toggle")
                .expect("the commit strip exposes bulk file folding");
            assert!(toggle.top() >= toolbar.top() && toggle.bottom() <= toolbar.bottom());
            vcx.simulate_click(toggle.center(), gpui::Modifiers::none());
            draw(vcx);
            let header = vcx.debug_bounds("diff-file-header:9").unwrap();
            assert!(
                header.top() >= viewport.top() && header.bottom() <= viewport.bottom(),
                "collapsing a commit keeps the reader's file visible in {mode:?}"
            );
            assert!(vcx.debug_bounds(text_selector).is_none());
            assert_eq!(vcx.debug_bounds("commit-diff-toolbar"), Some(toolbar));
            assert_eq!(vcx.debug_bounds("diff-commit-stats"), Some(totals));

            vcx.simulate_click(toggle.center(), gpui::Modifiers::none());
            draw(vcx);
            let reading = vcx.debug_bounds("diff-card-9").unwrap();
            assert!(
                reading.top() < viewport.bottom() && reading.bottom() > viewport.top(),
                "expanding a commit keeps the reader's file visible in {mode:?}"
            );
            assert!(vcx.debug_bounds(text_selector).is_some());
            assert_eq!(vcx.debug_bounds("commit-diff-toolbar"), Some(toolbar));
            vcx.update(|window, _| window.remove_window());
        }
    }

    #[gpui::test]
    fn focused_commit_controls_work_in_narrow_windows(cx: &mut TestAppContext) {
        let (app, mut initial_window) = crate::ui::app::test_window::harness(cx);
        initial_window.update(|window, _| window.remove_window());
        let (_, vcx) = cx.add_window_view(|_, _| CommitDocument(app.clone()));
        vcx.simulate_resize(size(px(560.), px(700.)));
        vcx.update(|_, cx| cx.global_mut::<Config>().diff_view = DiffViewMode::Split);

        let mut full = file("focused.rs", 100);
        full.status = FileStatus::Modified;
        full.added = 2;
        for line in &mut full.hunks[0].lines {
            if matches!(line.new_no, Some(25 | 65)) {
                continue;
            }
            line.kind = LineKind::Context;
            line.old_no = line.new_no;
        }
        let full = Arc::new(full);
        install(
            &app,
            vcx,
            vec![
                file("other.rs", 2),
                crate::ui::diff_context::with_context(&full, &[]),
            ],
        );
        app.update_in(vcx, |app, window, cx| {
            let overlay = app.tabs[0].diff_overlay.as_ref().unwrap();
            app.open_diff_overlay(
                overlay.host_id,
                overlay.cwd.clone(),
                overlay.source.clone(),
                Some("focused.rs".into()),
                window,
                cx,
            );
            app.tabs[0]
                .diff_overlay
                .as_mut()
                .unwrap()
                .file_views
                .insert(
                    "focused.rs".into(),
                    DiffFileView {
                        whole: false,
                        revealed: vec![],
                        full: Some(full),
                        shown: None,
                        loading: false,
                        failed: false,
                        token: Rc::new(()),
                    },
                );
            cx.notify();
        });
        draw(vcx);
        assert!(vcx.debug_bounds("diff-bulk-fold-toggle").is_none());
        assert!(vcx.debug_bounds("diff-card-0").is_none());

        for width in [560., 360.] {
            vcx.simulate_resize(size(px(width), px(700.)));
            draw(vcx);
            let toolbar = vcx.debug_bounds("commit-diff-toolbar").unwrap();
            let totals = vcx.debug_bounds("diff-commit-stats").unwrap();
            let mut previous_right = totals.right();
            for selector in [
                "diff-view-toggle",
                "diff-toolbar-context",
                "diff-previous-change",
                "diff-next-change",
            ] {
                let control = vcx
                    .debug_bounds(selector)
                    .expect("focused action stays visible");
                assert!(
                    control.left() >= previous_right
                        && control.right() <= toolbar.right()
                        && control.top() >= toolbar.top()
                        && control.bottom() <= toolbar.bottom(),
                    "{selector} must fit without overlap at {width}px: {control:?} / {toolbar:?}"
                );
                previous_right = control.right();
            }
            for selector in ["diff-unified-text", "diff-new-text"] {
                let toggle = vcx.debug_bounds("diff-view-toggle").unwrap();
                vcx.simulate_click(toggle.center(), gpui::Modifiers::none());
                draw(vcx);
                assert!(
                    vcx.debug_bounds(selector).is_some(),
                    "layout switches at {width}px"
                );
                assert!(vcx.debug_bounds("diff-card-0").is_none());
            }
        }

        let compact = vcx.debug_bounds("diff-card-1").unwrap();
        let context = vcx.debug_bounds("diff-toolbar-context").unwrap();
        vcx.simulate_click(context.center(), gpui::Modifiers::none());
        draw(vcx);
        let expanded = vcx.debug_bounds("diff-card-1").unwrap();
        assert!(expanded.size.height > compact.size.height + px(500.));
        let next = vcx.debug_bounds("diff-next-change").unwrap();
        vcx.simulate_click(next.center(), gpui::Modifiers::none());
        draw(vcx);
        let first_change = vcx.debug_bounds("diff-card-1").unwrap();
        vcx.simulate_click(next.center(), gpui::Modifiers::none());
        draw(vcx);
        let second_change = vcx.debug_bounds("diff-card-1").unwrap();
        assert!(second_change.top() < first_change.top() - px(500.));
        let previous = vcx.debug_bounds("diff-previous-change").unwrap();
        vcx.simulate_click(previous.center(), gpui::Modifiers::none());
        draw(vcx);
        let returned = vcx.debug_bounds("diff-card-1").unwrap();
        assert!((returned.top() - first_change.top()).abs() <= px(1.));

        let back = vcx.debug_bounds("commit-document-unfocus").unwrap();
        vcx.simulate_click(back.center(), gpui::Modifiers::none());
        draw(vcx);
        assert!(vcx.debug_bounds("diff-card-0").is_some());
        assert!(vcx.debug_bounds("diff-card-1").is_some());
        assert!(vcx.debug_bounds("diff-toolbar-context").is_none());
        assert!(vcx.debug_bounds("diff-previous-change").is_none());
        assert!(vcx.debug_bounds("diff-next-change").is_none());
        let bulk = vcx.debug_bounds("diff-bulk-fold-toggle").unwrap();
        vcx.simulate_click(bulk.center(), gpui::Modifiers::none());
        draw(vcx);
        assert!(vcx.debug_bounds("diff-new-text").is_none());
        vcx.simulate_click(bulk.center(), gpui::Modifiers::none());
        draw(vcx);
        assert!(vcx.debug_bounds("diff-new-text").is_some());
        let viewport = vcx.debug_bounds("diff-scroll").unwrap();
        let reading = vcx.debug_bounds("diff-card-1").unwrap();
        assert!(reading.top() < viewport.bottom() && reading.bottom() > viewport.top());
    }
}
