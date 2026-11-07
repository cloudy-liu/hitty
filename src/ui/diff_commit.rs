//! The selected commit's message and metadata above the existing diff content.
use super::*;
use gpui::{Bounds, MouseButton, MouseMoveEvent, MouseUpEvent};
use gpui_component::InteractiveElementExt as _;
use std::cell::Cell;
use tty7_core::core::date::format_date;

impl Tty7App {
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
                                    if reference.kind == tty7_core::core::git::log::RefKind::Tag {
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
        if let DiffLoad::Ready(snapshot) = &overlay.load {
            let (added, removed) = snapshot.stats().totals;
            details = details.child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .debug_selector(|| "diff-commit-stats".into())
                    .text_xs()
                    .child(Icon::new(IconName::Folder).small().text_color(muted))
                    .child(div().text_color(muted).child(t_plural(
                        L10nKey::DiffChangedFiles,
                        snapshot.files.len(),
                        &[],
                    )))
                    .child(
                        div()
                            .font_family(mono.clone())
                            .text_color(cx.theme().success)
                            .child(format!("+{added}")),
                    )
                    .child(
                        div()
                            .font_family(mono)
                            .text_color(cx.theme().danger)
                            .child(format!("\u{2212}{removed}")),
                    ),
            );
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

    pub(super) fn divider(&self, cx: &mut Context<Tty7App>) -> AnyElement {
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
