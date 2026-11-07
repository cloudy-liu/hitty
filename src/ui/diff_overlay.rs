use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Weak};

#[path = "diff_commit.rs"]
mod commit;
pub(crate) use commit::CommitSplit;

#[cfg(feature = "markdown-visual-tests")]
#[path = "diff_profile.rs"]
pub(crate) mod profile;

use gpui::{
    AnyElement, Background, FocusHandle, FontWeight, Hsla, KeyDownEvent, Pixels, SharedString,
    Window, div, prelude::*, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::menu::ContextMenuExt as _;
use gpui_component::scroll::{Scrollbar, ScrollbarShow};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, h_flex, v_flex,
};

use crate::core::config::{Config, DiffViewMode};
use crate::core::git::status::DecoStatus;
use crate::terminal::git_diff::{
    self, AUTO_COLLAPSE_LINES, CommitLabel, DiffSnapshot, DiffSource, DiffStats, FileDiff,
    FileStatus, LineKind, MAX_RENDERED_FILES, Truncation,
};

/// How much of an untracked file the preview will read. Past this the card
/// says the read failed rather than showing a silently cut-off file — and the
/// line budget below cuts rendering long before this does anyway.
const MAX_PREVIEW_BYTES: u64 = 4 * 1024 * 1024;
use crate::ui::app::Tty7App;
use crate::ui::diff_rows::{Side, SplitCell, SplitRow, UnifiedRow, split_hunk, unified_rows};
use crate::ui::diff_selection::{
    DiffSelection, SelectableLabel, SelectionLine, selectable_card, selectable_viewport,
};
use crate::ui::document_column::DocumentChrome;
use crate::ui::i18n::{L10nKey, t, t_fmt, t_plural};
use crate::ui::right_panel::info_chip;
use crate::ui::rounding;
use crate::ui::rounding::RoundedCorners as _;
use crate::ui::scm::path::relative_time;
use crate::ui::scm::status::status_glyph;

const SPLIT_GUTTER_WIDTH: f32 = 42.;
const UNIFIED_GUTTER_WIDTH: f32 = 34.;
const DIFF_MARKER_WIDTH: f32 = 12.;
const DIFF_DIVIDER_WIDTH: f32 = 1.;
const DIFF_LINE_END_PADDING: f32 = 4.;
const DIFF_ROW_HEIGHT: f32 = 19.;
const DIFF_HUNK_CONTROL_WIDTH: f32 = 42.;

/// Reserve the complete hunk height, but build and lay out only clipped-in rows.
/// The outer file list still owns scrolling, including selection autoscroll.
fn visible_diff_rows(
    count: usize,
    min_width: f32,
    render: impl 'static + FnOnce(std::ops::Range<usize>, &mut Window, &mut gpui::App) -> AnyElement,
) -> impl IntoElement {
    gpui::canvas(
        move |bounds, window, cx| {
            let visible = bounds.intersect(&window.content_mask().bounds);
            if visible.size.width <= px(0.) || visible.size.height <= px(0.) {
                return None;
            }
            let first = ((visible.top() - bounds.top()).as_f32() / DIFF_ROW_HEIGHT)
                .floor()
                .max(0.) as usize;
            let end = (((visible.bottom() - bounds.top()).as_f32() / DIFF_ROW_HEIGHT).ceil()
                as usize)
                .min(count);
            let mut rows = render(first..end, window, cx);
            rows.layout_as_root(
                gpui::size(
                    gpui::AvailableSpace::Definite(bounds.size.width),
                    gpui::AvailableSpace::Definite(px((end - first) as f32 * DIFF_ROW_HEIGHT)),
                ),
                window,
                cx,
            );
            rows.prepaint_at(
                gpui::point(
                    bounds.left(),
                    bounds.top() + px(first as f32 * DIFF_ROW_HEIGHT),
                ),
                window,
                cx,
            );
            Some(rows)
        },
        |_, rows, window, cx| {
            if let Some(mut rows) = rows {
                rows.paint(window, cx);
            }
        },
    )
    .w_full()
    .min_w(px(min_width.ceil()))
    .h(px(count as f32 * DIFF_ROW_HEIGHT))
    .flex_shrink_0()
}

pub(crate) enum DiffLoad {
    Loading,
    Unavailable,
    Ready(Arc<DiffSnapshot>),
    NotARepo,
}

pub(crate) struct DiffOverlayState {
    pub(crate) host_id: crate::ui::host_ops::HostId,
    /// Native pane lifetimes differ from persistent workspace machines. This
    /// weak handle lets inactive overlays discard data as soon as a pane ends.
    pub(crate) pane_host: Option<Weak<dyn crate::ui::host_ops::Host>>,
    pub(crate) cwd: PathBuf,
    /// Which patch this overlay is showing. Part of its identity, not a
    /// setting: two sources over one directory are two different overlays.
    pub(crate) source: DiffSource,
    pub(crate) commit: Option<Arc<tty7_core::core::git::log::Commit>>,
    pub(crate) commit_message_expanded: bool,
    pub(crate) commit_split: CommitSplit,
    pub(crate) document_header_focus: FocusHandle,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) load: DiffLoad,
    pub(crate) loading: bool,
    pub(crate) expanded: HashMap<String, bool>,
    pub(crate) focus: Option<String>,
    /// A synthesized all-added card for a focused *untracked* file, keyed by
    /// path; `None` in the value means the read failed. git has no patch for
    /// an untracked file, so focusing one reads its bytes instead — lazily,
    /// only for the file on screen, never for the whole list. Cleared when a
    /// fresh snapshot lands, so an edit to the file shows up on the same
    /// cadence a tracked file's does.
    pub(crate) preview: Option<(String, Option<Arc<FileDiff>>)>,
    pub(crate) preview_loading: Option<String>,
    pub(crate) scroll: gpui::ScrollHandle,
    /// The [`ScmData`](crate::terminal::git_data::ScmData) epoch this patch was
    /// read at, for the two sources that can go stale.
    ///
    /// Recorded when a probe *starts*, so a `git add` that lands while one is
    /// running is not mistaken for a change the result already reflects.
    /// `None` until the first snapshot arrives: the epoch is keyed by the
    /// repository root, and only a snapshot knows where that is.
    pub(crate) epoch: Option<u64>,
    pub(crate) file_views: HashMap<String, DiffFileView>,
    pub(crate) geometry: Rc<RefCell<HashMap<String, DiffGeometry>>>,
    pub(crate) anchor: Rc<RefCell<Option<DiffAnchor>>>,
}

pub(crate) struct DiffFileView {
    whole: bool,
    revealed: Vec<Range<u32>>,
    full: Option<Arc<FileDiff>>,
    shown: Option<Arc<FileDiff>>,
    loading: bool,
    failed: bool,
    token: Rc<()>,
}

impl DiffFileView {
    fn update_shown(&mut self) {
        self.shown = self.full.as_ref().map(|full| {
            if self.whole {
                full.clone()
            } else {
                Arc::new(crate::ui::diff_context::with_context(full, &self.revealed))
            }
        });
    }
}

type DiffLineKey = (Option<u32>, Option<u32>);
pub(crate) struct DiffGeometry {
    content_top: f32,
    rows: Arc<Vec<(DiffLineKey, f32, bool)>>,
}
pub(crate) struct DiffAnchor {
    path: String,
    line: DiffLineKey,
    screen_y: f32,
}

fn anchor_row(
    rows: &[(DiffLineKey, f32, bool)],
    line: DiffLineKey,
) -> Option<&(DiffLineKey, f32, bool)> {
    rows.iter()
        .find(|(key, _, _)| *key == line)
        .or_else(|| {
            rows.iter().find(|(key, _, _)| {
                (line.1.is_some() && key.1 == line.1) || (line.0.is_some() && key.0 == line.0)
            })
        })
        .or_else(|| {
            rows.iter().min_by_key(|(key, _, _)| {
                key.1
                    .or(key.0)
                    .unwrap_or(0)
                    .abs_diff(line.1.or(line.0).unwrap_or(0))
            })
        })
}

impl DiffOverlayState {
    fn pane_disconnected(&self) -> bool {
        self.pane_host
            .as_ref()
            .is_some_and(|host| host.upgrade().is_none_or(|host| !host.is_connected()))
    }

    fn remember_diff_position(&self, path: &str) {
        let geometry = self.geometry.borrow();
        let Some(g) = geometry.get(path) else {
            return;
        };
        let top = self.scroll.bounds().top().as_f32();
        let screen_top = g.content_top + self.scroll.offset().y.as_f32();
        if let Some((line, y, _)) = g.rows.iter().find(|(_, y, _)| screen_top + y >= top) {
            *self.anchor.borrow_mut() = Some(DiffAnchor {
                path: path.to_string(),
                line: *line,
                screen_y: screen_top + y,
            });
        }
    }
}

/// One hunk, already turned into whichever kind of row the current view draws.
enum HunkRows {
    Split(Vec<SplitRow>),
    Unified(Vec<UnifiedRow>),
}

impl HunkRows {
    fn len(&self) -> usize {
        match self {
            HunkRows::Split(rows) => rows.len(),
            HunkRows::Unified(rows) => rows.len(),
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Paints the full-window diff surface without inheriting workspace opacity.
/// The preset's solid or gradient design remains intact, but neither it nor
/// the plain theme fallback may reveal the OS backdrop through diff text.
fn diff_overlay_background(
    active: Option<&crate::ui::presets::ActiveBackground>,
    fallback: Hsla,
) -> Background {
    match active {
        Some(bg) => crate::ui::theme::window_background_opaque(bg),
        None => fallback.alpha(1.0).into(),
    }
}

// Reading surfaces share the application's background, text and hairlines, as
// the bundled Markdown theme does. Change colors come from the immutable
// bundled GitHub palette, independent of the user's Markdown theme choice and
// application preset status colors.
struct DiffPalette {
    background: Hsla,
    foreground: Hsla,
    header: Hsla,
    hunk_background: Hsla,
    hunk_controls: Hsla,
    muted: Hsla,
    border: Hsla,
    hunk: Hsla,
    added: Hsla,
    added_background: Hsla,
    removed: Hsla,
    removed_background: Hsla,
    changed: Hsla,
}

fn diff_palette(cx: &gpui::App) -> DiffPalette {
    let theme = crate::core::markdown_theme::builtin();
    let p = theme.palette(cx.theme().mode.is_dark());
    let color = crate::ui::markdown_preview::color;
    let added = p
        .syntax
        .diff_added
        .as_ref()
        .expect("bundled GitHub additions");
    let removed = p
        .syntax
        .diff_deleted
        .as_ref()
        .expect("bundled GitHub deletions");
    let dark = cx.theme().mode.is_dark();
    DiffPalette {
        background: gpui::rgb(cx.global::<crate::ui::presets::Surfaces>().window.base).into(),
        foreground: cx.theme().foreground,
        header: cx.theme().popover,
        hunk_background: gpui::rgb(if dark { 0x1b_3040 } else { 0xdd_f4ff }).into(),
        hunk_controls: gpui::rgb(if dark { 0x2a_5a_7a } else { 0xb6_e3ff }).into(),
        muted: cx.theme().muted_foreground,
        border: cx.theme().border,
        hunk: color(p.syntax.diff_hunk.expect("bundled GitHub hunk color")),
        added: color(added.foreground),
        added_background: color(
            added
                .background
                .expect("bundled GitHub addition background"),
        ),
        removed: color(removed.foreground),
        removed_background: color(
            removed
                .background
                .expect("bundled GitHub deletion background"),
        ),
        changed: color(
            p.syntax
                .diff_changed
                .as_ref()
                .expect("bundled GitHub changes")
                .foreground,
        ),
    }
}

impl Tty7App {
    fn change_diff_context(
        &mut self,
        path: String,
        reveal: Option<Range<u32>>,
        cx: &mut Context<Self>,
    ) {
        let Some(o) = self
            .tabs
            .get_mut(self.active)
            .and_then(|t| t.diff_overlay.as_mut())
        else {
            return;
        };
        let DiffLoad::Ready(snap) = &o.load else {
            return;
        };
        let Some(file) = snap.files.iter().find(|f| f.path == path && !f.binary) else {
            return;
        };
        o.remember_diff_position(&path);
        let view = o
            .file_views
            .entry(path.clone())
            .or_insert_with(|| DiffFileView {
                whole: false,
                revealed: Vec::new(),
                full: None,
                shown: None,
                loading: false,
                failed: false,
                token: Rc::new(()),
            });
        if view.loading {
            return;
        }
        if let Some(range) = reveal {
            view.whole = false;
            view.revealed.push(range);
        } else {
            view.whole = !view.whole || view.failed;
            view.revealed.clear();
        }
        o.expanded.insert(path.clone(), true);
        if view.full.is_some() {
            view.update_shown();
            cx.notify();
            return;
        }
        let Some(host) = crate::ui::host_registry::HostRegistry::lookup(cx, o.host_id) else {
            return;
        };
        let root = snap.root.clone();
        let source = o.source.clone();
        let file = file.clone();
        let token = view.token.clone();
        let landing_host = host.clone();
        let native_pane =
            crate::ui::host_registry::HostRegistry::pane_lifetime(cx, host.id()).is_some();
        view.loading = true;
        view.failed = false;
        crate::ui::host_ops::HostOps::run(
            host,
            cx,
            move |h| git_diff::probe_full_file(h, &root, source, &file).map(Arc::new),
            move |app, full, cx| {
                if native_pane && !landing_host.is_connected() {
                    app.retire_disconnected_pane_diffs(cx);
                    return;
                }
                for tab in &mut app.tabs {
                    let Some(o) = tab.diff_overlay.as_mut() else {
                        continue;
                    };
                    let Some(view) = o.file_views.get(&path) else {
                        continue;
                    };
                    if !Rc::ptr_eq(&view.token, &token) {
                        continue;
                    }
                    o.remember_diff_position(&path);
                    let view = o.file_views.get_mut(&path).unwrap();
                    view.loading = false;
                    view.failed = full.is_none();
                    view.full = full.clone();
                    view.update_shown();
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    fn navigate_diff_change(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(o) = self
            .tabs
            .get(self.active)
            .and_then(|t| t.diff_overlay.as_ref())
        else {
            return;
        };
        let Some(path) = o.focus.as_ref() else {
            return;
        };
        let geometry = o.geometry.borrow();
        let Some(g) = geometry.get(path) else {
            return;
        };
        let top = o.scroll.bounds().top().as_f32() + 1.;
        let mut previous_changed = false;
        let starts: Vec<_> = g
            .rows
            .iter()
            .filter_map(|(_, y, changed)| {
                let start = *changed && !previous_changed;
                previous_changed = *changed;
                start.then_some(g.content_top + o.scroll.offset().y.as_f32() + y)
            })
            .collect();
        let target = if forward {
            starts
                .iter()
                .find(|y| **y > top + DIFF_ROW_HEIGHT)
                .or(starts.first())
        } else {
            starts
                .iter()
                .rev()
                .find(|y| **y < top - DIFF_ROW_HEIGHT)
                .or(starts.last())
        };
        if let Some(y) = target {
            let mut offset = o.scroll.offset();
            offset.y += px(top - y);
            o.scroll.set_offset(offset);
            cx.notify();
        }
    }

    fn diff_context_button(&self, path: &str, cx: &mut Context<Self>) -> AnyElement {
        let view = self
            .tabs
            .get(self.active)
            .and_then(|t| t.diff_overlay.as_ref())
            .and_then(|o| o.file_views.get(path));
        let whole = view.is_some_and(|v| v.whole && !v.failed);
        let loading = view.is_some_and(|v| v.loading);
        let selector = format!("diff-context-button:{path}");
        let path = path.to_string();
        div()
            .debug_selector(move || selector.clone())
            .occlude()
            .flex_shrink_0()
            .child(
                Button::new(SharedString::from(format!("diff-context:{path}")))
                    .small()
                    .ghost()
                    .disabled(loading)
                    .child(
                        gpui::svg()
                            .path(if whole {
                                "icons/diff-fold.svg"
                            } else {
                                "icons/diff-unfold.svg"
                            })
                            .size(px(14.))
                            .text_color(cx.theme().foreground),
                    )
                    .tooltip(t(if whole {
                        L10nKey::DiffCollapseContext
                    } else {
                        L10nKey::DiffExpandFile
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.change_diff_context(path.clone(), None, cx);
                    })),
            )
            .into_any_element()
    }

    fn diff_change_button(&self, forward: bool, cx: &mut Context<Self>) -> AnyElement {
        let has_changes = self
            .tabs
            .get(self.active)
            .and_then(|t| t.diff_overlay.as_ref())
            .is_some_and(|o| {
                matches!(&o.load, DiffLoad::Ready(s) if s.files.iter().any(|f|
                o.focus.as_deref() == Some(f.path.as_str()) && (f.added > 0 || f.removed > 0)))
            });
        div()
            .occlude()
            .flex_shrink_0()
            .child(
                Button::new(if forward {
                    "diff-next-change"
                } else {
                    "diff-previous-change"
                })
                .small()
                .ghost()
                .disabled(!has_changes)
                .icon(if forward {
                    IconName::ArrowDown
                } else {
                    IconName::ArrowUp
                })
                .tooltip(t(if forward {
                    L10nKey::DiffNextChange
                } else {
                    L10nKey::DiffPreviousChange
                }))
                .on_click(
                    cx.listener(move |this, _, _, cx| this.navigate_diff_change(forward, cx)),
                ),
            )
            .into_any_element()
    }

    fn diff_more_button(
        &self,
        path: &str,
        range: Range<u32>,
        above: Option<bool>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selector = format!("diff-more:{path}:{}:{}", range.start, range.end);
        let path = path.to_string();
        let button = Button::new(SharedString::from(format!(
            "diff-more:{path}:{}:{}",
            range.start, range.end
        )))
        .xsmall()
        .ghost()
        .w_full()
        .h_full()
        .p_0()
        .rounded_none()
        .text_color(diff_palette(cx).muted)
        .child(
            gpui::svg()
                .path(match above {
                    Some(true) => "icons/diff-expand-up.svg",
                    Some(false) => "icons/diff-expand-down.svg",
                    None => "icons/diff-unfold.svg",
                })
                .size(px(16.))
                .text_color(diff_palette(cx).muted),
        )
        .tooltip(t(match above {
            Some(true) => L10nKey::DiffMoreAbove,
            Some(false) => L10nKey::DiffMoreBelow,
            None => L10nKey::DiffExpandGap,
        }))
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.change_diff_context(path.clone(), Some(range.clone()), cx);
        }));
        div()
            .debug_selector(move || selector.clone())
            .w_full()
            .flex_1()
            .min_h_0()
            .child(button)
            .into_any_element()
    }

    pub(crate) fn toggle_diff_overlay(
        &mut self,
        host: crate::ui::host_ops::HostId,
        cwd: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_diff_overlay_at(host, cwd, None, window, cx)
    }

    pub(crate) fn toggle_diff_overlay_at(
        &mut self,
        host: crate::ui::host_ops::HostId,
        cwd: PathBuf,
        focus: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The sidebar's `+N −M` is `diff --numstat HEAD`, so opening it has
        // to show the same span. The panel names its own source per group.
        self.open_diff_overlay(host, cwd, DiffSource::Head, focus, window, cx)
    }

    pub(crate) fn open_diff_overlay(
        &mut self,
        host: crate::ui::host_ops::HostId,
        cwd: PathBuf,
        source: DiffSource,
        focus: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active = self.active;
        let commit_document = matches!(&source, DiffSource::Commit { label: Some(_), .. });
        let commit = self
            .scm
            .detail
            .as_ref()
            .filter(|detail| {
                detail.repo.host == host
                    && detail.repo.root == cwd
                    && matches!(&source, DiffSource::Commit { rev, .. } if *rev == detail.oid)
            })
            .and_then(|detail| detail.commit.clone());
        let was_front = self.tabs.get(active).is_some_and(|t| {
            t.overlay_top == crate::ui::app::OverlayTop::Diff || !self.code_panel_visible()
        });
        if let Some(tab) = self.tabs.get_mut(active) {
            tab.overlay_top = crate::ui::app::OverlayTop::Diff;
        }
        // The source belongs in this filter: an open worktree overlay reused
        // for a staged file would only move the focus, and go on showing the
        // unstaged patch under the staged file's name.
        match self
            .tabs
            .get_mut(active)
            .and_then(|t| t.diff_overlay.as_mut())
            .filter(|o| o.cwd == cwd && o.host_id == host && o.source == source)
        {
            Some(o) if o.focus == focus && was_front => {
                self.close_diff_overlay(window, cx);
                return;
            }
            Some(o) => {
                if commit.is_some() {
                    o.commit = commit;
                }
                let refresh_native = o.pane_host.is_some();
                let pending = o.loading;
                o.file_views.clear();
                o.focus = focus;
                *o.anchor.borrow_mut() = None;
                let handle = o.focus_handle.clone();
                window.focus(&handle, cx);
                if commit_document {
                    self.reveal_commit_document_tab();
                }
                if refresh_native {
                    if pending {
                        self.diff_probes_restale
                            .insert(probe_key(host, &cwd, &source));
                    } else {
                        self.spawn_diff_probe(cx);
                    }
                }
                cx.notify();
                return;
            }
            None => {}
        }
        self.remember_active_pane(window, cx);
        let Some(tab) = self.tabs.get_mut(active) else {
            return;
        };
        let focus_handle = cx.focus_handle();
        let pane_host = crate::ui::host_registry::HostRegistry::pane_lifetime(cx, host);
        tab.diff_overlay = Some(DiffOverlayState {
            host_id: host,
            pane_host,
            cwd,
            source,
            commit,
            commit_message_expanded: false,
            commit_split: crate::ui::diff_overlay::CommitSplit::new(cx),
            document_header_focus: cx.focus_handle(),
            focus_handle: focus_handle.clone(),
            // Every open starts at Loading until its own probe lands. The old
            // panel-snapshot seeding died with the panel that held a snapshot
            // per source; re-seeding would need the caller to carry one.
            load: DiffLoad::Loading,
            loading: false,
            expanded: HashMap::new(),
            focus,
            preview: None,
            preview_loading: None,
            scroll: gpui::ScrollHandle::new(),
            epoch: None,
            file_views: HashMap::new(),
            geometry: Rc::default(),
            anchor: Rc::default(),
        });
        window.focus(&focus_handle, cx);
        if commit_document {
            self.reveal_commit_document_tab();
        }
        self.spawn_diff_probe(cx);
        cx.notify();
    }

    /// Which file the open overlay is focused on — for the row that asked,
    /// which means the *source* has to match too: a file staged and edited
    /// again sits in two panel groups, and only the row whose patch is
    /// actually on screen may draw itself selected.
    pub(crate) fn diff_overlay_focus(
        &self,
        host: crate::ui::host_ops::HostId,
        cwd: &std::path::Path,
        source: &crate::terminal::git_diff::DiffSource,
    ) -> Option<&str> {
        let overlay = self.tabs.get(self.active)?.diff_overlay.as_ref()?;
        (overlay.cwd == cwd && overlay.host_id == host && overlay.source == *source)
            .then_some(overlay.focus.as_deref())?
    }

    pub(crate) fn close_diff_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.active;
        let taken = self
            .tabs
            .get_mut(active)
            .and_then(|t| t.diff_overlay.take());
        if taken.is_some() {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    fn spawn_diff_probe(&mut self, cx: &mut Context<Self>) {
        let active = self.active;
        let Some(overlay) = self.tabs.get(active).and_then(|t| t.diff_overlay.as_ref()) else {
            return;
        };
        if overlay.loading {
            return;
        }
        let cwd = overlay.cwd.clone();
        let source = overlay.source.clone();
        let id = overlay.host_id;
        // Read before the probe is dispatched, not after it lands: anything
        // bumped in between belongs to the next read, not this one.
        let epoch = match &overlay.load {
            DiffLoad::Ready(snap) => Some(scm_epoch(cx, id, &snap.root)),
            _ => None,
        };
        let Some(host) = crate::ui::host_registry::HostRegistry::lookup(cx, id) else {
            if let Some(overlay) = self
                .tabs
                .get_mut(active)
                .and_then(|t| t.diff_overlay.as_mut())
            {
                overlay.loading = false;
                overlay.load = DiffLoad::Unavailable;
                cx.notify();
            }
            return;
        };
        let Some(overlay) = self
            .tabs
            .get_mut(active)
            .and_then(|t| t.diff_overlay.as_mut())
        else {
            return;
        };
        overlay.loading = true;
        overlay.file_views.clear();
        *overlay.anchor.borrow_mut() = None;
        overlay.epoch = epoch;
        self.spawn_diff_probe_for(host, cwd, source, cx);
    }

    pub(crate) fn spawn_diff_probe_for(
        &mut self,
        host: crate::ui::host_ops::SharedHost,
        cwd: PathBuf,
        source: DiffSource,
        cx: &mut Context<Self>,
    ) {
        let key = probe_key(host.id(), &cwd, &source);
        if !self.diff_probes_inflight.insert(key.clone()) {
            self.diff_probes_restale.insert(key);
            return;
        }
        let native_pane =
            crate::ui::host_registry::HostRegistry::pane_lifetime(cx, host.id()).is_some();
        let host_for_retry = host.clone();
        let probe_cwd = cwd.clone();
        let probe_source = source.clone();
        crate::ui::host_ops::HostOps::run(
            host,
            cx,
            move |h| {
                let req = git_diff::DiffRequest {
                    source: probe_source,
                    ..Default::default()
                };
                git_diff::probe_diff(h, &probe_cwd, &req)
            },
            move |app, result, cx| {
                let id = key.0;
                app.diff_probes_inflight.remove(&key);
                if native_pane && !host_for_retry.is_connected() {
                    app.diff_probes_restale.remove(&key);
                    app.retire_disconnected_pane_diffs(cx);
                    return;
                }
                app.install_diff_snapshot(id, &cwd, &source, result.map(Arc::new), cx);
                if app.diff_probes_restale.remove(&key) {
                    app.spawn_diff_probe_for(host_for_retry, cwd, source, cx);
                }
            },
        );
    }

    fn install_diff_snapshot(
        &mut self,
        host: crate::ui::host_ops::HostId,
        cwd: &Path,
        source: &DiffSource,
        snap: Option<Arc<DiffSnapshot>>,
        cx: &mut Context<Self>,
    ) {
        // A diff read is a fresher answer to the question the sidebar's branch
        // and +N −N ask, and it is the one the reader is looking at. Hand the
        // branch and the numbers back before anything renders, or the row can
        // disagree with the overlay it just opened — and `maybe_refresh` reads
        // that disagreement as a reason to probe again.
        //
        // A read that failed is not an answer at all: its totals are whatever
        // got parsed before git gave up, usually zero. Publishing those wipes
        // the counts the sidebar already had right — the overlay says so in
        // words a few lines below, and the row would silently disagree.
        let mut landed = if let Some(snap) = snap.as_ref().filter(|s| !s.read_failed) {
            // Only a HEAD snapshot counts what the sidebar counts. A worktree
            // or staged patch is a smaller answer to a different question, and
            // a commit or a range is not about the working tree at all.
            let counts = matches!(snap.source, DiffSource::Head).then(|| snap.totals());
            let root = snap.root.clone();
            let branch = snap.branch.clone();
            cx.default_global::<crate::terminal::git_status::GitStatusCache>();
            cx.update_global::<crate::terminal::git_status::GitStatusCache, _>(|cache, _| {
                cache.note_diff_read(host, &root, &branch, counts)
            })
        } else {
            false
        };
        // Only wanted by an overlay whose first probe could not know the root,
        // and so could not read its own epoch before dispatching.
        let landing_epoch = snap.as_ref().map(|s| scm_epoch(cx, host, &s.root));
        let mut reveal_commit_tab = false;
        let active = self.active;
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            let front = index == active && tab.overlay_top == crate::ui::app::OverlayTop::Diff;
            let Some(overlay) = tab
                .diff_overlay
                .as_mut()
                .filter(|o| o.cwd == cwd && o.host_id == host && o.source == *source)
            else {
                continue;
            };
            let previously_had_actions = commit_has_diff_actions(overlay);
            overlay.loading = false;
            overlay.epoch = overlay.epoch.or(landing_epoch);
            overlay.load = match &snap {
                Some(snap) => DiffLoad::Ready(Arc::clone(snap)),
                None => DiffLoad::NotARepo,
            };
            if front
                && matches!(overlay.source, DiffSource::Commit { label: Some(_), .. })
                && !previously_had_actions
                && commit_has_diff_actions(overlay)
            {
                reveal_commit_tab = true;
            }
            // A new snapshot restarts any untracked preview: the file may
            // have changed with the tree, and the re-read costs one file.
            overlay.preview = None;
            overlay.file_views.clear();
            *overlay.anchor.borrow_mut() = None;
            landed = true;
        }
        if reveal_commit_tab {
            self.reveal_commit_document_tab();
        }
        if landed {
            cx.notify();
        }
    }

    fn retire_disconnected_pane_diffs(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        let mut retired = Vec::new();
        for tab in &mut self.tabs {
            let Some(overlay) = &mut tab.diff_overlay else {
                continue;
            };
            if !overlay.pane_disconnected() {
                continue;
            }
            retired.push(overlay.host_id);
            if !matches!(overlay.load, DiffLoad::Unavailable)
                || overlay.loading
                || overlay.preview.is_some()
                || overlay.preview_loading.is_some()
                || !overlay.file_views.is_empty()
            {
                overlay.load = DiffLoad::Unavailable;
                overlay.loading = false;
                overlay.preview = None;
                overlay.preview_loading = None;
                overlay.file_views.clear();
                overlay.epoch = None;
                changed = true;
            }
        }
        self.diff_probes_inflight
            .retain(|key| !retired.contains(&key.0));
        self.diff_probes_restale
            .retain(|key| !retired.contains(&key.0));
        if changed {
            cx.notify();
        }
    }

    pub(crate) fn maybe_refresh_diff_overlay(&mut self, cx: &mut Context<Self>) {
        self.retire_disconnected_pane_diffs(cx);
        let Some(overlay) = self
            .tabs
            .get(self.active)
            .and_then(|t| t.diff_overlay.as_ref())
        else {
            return;
        };
        if overlay.loading {
            return;
        }
        let DiffLoad::Ready(snap) = &overlay.load else {
            return;
        };
        let stale = match overlay.source {
            // A commit and a range are fixed patches. Nothing can make either
            // of them out of date, so nothing should reprobe them.
            DiffSource::Commit { .. } | DiffSource::Range { .. } => return,
            // The cached counts come from `git diff --numstat HEAD`, so only a
            // HEAD snapshot is comparable to them.
            DiffSource::Head => {
                let Some(status) = cx
                    .try_global::<crate::terminal::git_status::GitStatusCache>()
                    .and_then(|cache| cache.status_for(overlay.host_id, &overlay.cwd))
                else {
                    return;
                };
                status.branch != snap.branch
                    || (status.added, status.removed) != snap.totals()
                    || (overlay.pane_host.is_some()
                        && overlay
                            .epoch
                            .is_some_and(|seen| scm_epoch(cx, overlay.host_id, &snap.root) != seen))
            }
            // Those same counts would differ from a staged or unstaged patch
            // the moment anything is staged, and the overlay would reprobe
            // forever. The epoch answers the question that was actually being
            // asked — "did anything happen to this repository" — without
            // knowing what either side is counting.
            DiffSource::Worktree | DiffSource::Staged => {
                let Some(seen) = overlay.epoch else {
                    return;
                };
                scm_epoch(cx, overlay.host_id, &snap.root) != seen
            }
        };
        if stale {
            self.spawn_diff_probe(cx);
        }
    }

    pub(crate) fn render_diff_overlay(
        &mut self,
        chrome: DocumentChrome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.has_commit_document()
            && self.document_front() == Some(crate::ui::app::OverlayTop::Code)
        {
            return None;
        }
        self.spawn_untracked_preview_if_needed(cx);
        let overlay = self.tabs.get(self.active)?.diff_overlay.as_ref()?;

        let content = match &overlay.load {
            DiffLoad::Loading => self.diff_message(t(L10nKey::DiffReading), cx),
            DiffLoad::Unavailable => self.diff_message(t(L10nKey::DiffReadFailed), cx),
            DiffLoad::NotARepo => self.diff_message(t(L10nKey::DiffNotARepo), cx),
            DiffLoad::Ready(snap) if empty_snapshot(snap) && snap.read_failed => {
                self.diff_message(t(L10nKey::DiffReadFailed), cx)
            }
            DiffLoad::Ready(snap) if empty_snapshot(snap) => {
                self.diff_message(t(L10nKey::DiffWorkingTreeClean), cx)
            }
            // A focused *untracked* file has no patch in the snapshot; its
            // card is synthesized from the file's own bytes — see `preview`.
            DiffLoad::Ready(snap) if untracked_focus(snap, overlay.focus.as_deref()).is_some() => {
                let path = untracked_focus(snap, overlay.focus.as_deref()).unwrap();
                match &overlay.preview {
                    Some((held, Some(file))) if held == path => {
                        self.diff_preview_card(file.as_ref(), &overlay.scroll, window, cx)
                    }
                    Some((held, None)) if held == path => {
                        self.diff_message(t(L10nKey::DiffReadFailed), cx)
                    }
                    _ => self.diff_message(t(L10nKey::DiffReading), cx),
                }
            }
            DiffLoad::Ready(snap) => self.diff_file_list(
                snap,
                &overlay.expanded,
                focused_file(snap, overlay),
                &overlay.scroll,
                window,
                cx,
            ),
        };

        let palette = diff_palette(cx);
        let content = v_flex()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .bg(palette.background)
            .text_color(palette.foreground)
            .children(self.diff_commit_details(overlay, cx))
            .when(self.has_commit_document(), |body| {
                body.child(overlay.commit_split.backing()).child(
                    div()
                        .relative()
                        .flex_none()
                        .h(px(1.))
                        .child(overlay.commit_split.divider(cx)),
                )
            })
            .child(content);

        let commit_document = self.has_commit_document();
        let header = chrome.renders_own_header().then(|| {
            if commit_document {
                self.render_editor_header_only(chrome, window, cx)
            } else {
                self.diff_header(overlay, chrome, window, cx)
                    .into_any_element()
            }
        });
        let focus_handle = overlay.focus_handle.clone();

        let shell = v_flex().debug_selector(|| "diff-panel".into());
        let shell = match chrome {
            DocumentChrome::Fill => shell
                .absolute()
                .inset_0()
                .occlude()
                // Opaque on purpose: this overlay covers the entire workspace,
                // so window translucency and backdrop material must stop here.
                .bg(diff_overlay_background(
                    cx.try_global::<crate::ui::presets::ActiveBackground>(),
                    cx.theme().background,
                ))
                // The opaque fill above covers the theme background image the
                // workspace root paints, so the overlay carries its own copy,
                // dimmed back to the strength it had when this overlay was
                // itself translucent.
                .children(crate::ui::app::overlay_surface_layers(cx)),
            // Docked, the column wrapper has already painted the surface this
            // sits on — the same one the right panel uses — and nothing behind
            // it needs stopping.
            DocumentChrome::Dock | DocumentChrome::DockHoisted => shell.size_full().min_w_0(),
        };
        let shell = shell.when(
            !chrome.is_dock() && self.document_header_below_chrome(window, cx),
            |shell| shell.pt(px(crate::ui::app::TITLE_BAR_HEIGHT)),
        );
        Some(
            shell
                .text_color(cx.theme().foreground)
                .track_focus(&focus_handle)
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    if ev.keystroke.key.as_str() == "escape" {
                        this.close_diff_overlay(window, cx);
                    }
                }))
                .children(header)
                .children(commit_document.then(|| self.commit_document_breadcrumb(overlay, cx)))
                .child(content)
                .into_any_element(),
        )
    }

    /// The diff header alone, for the strip above a docked column.
    pub(crate) fn render_diff_header_only(
        &mut self,
        chrome: DocumentChrome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.has_commit_document() {
            return Some(self.render_editor_header_only(chrome, window, cx));
        }
        let overlay = self.tabs.get(self.active)?.diff_overlay.as_ref()?;
        Some(
            self.diff_header(overlay, chrome, window, cx)
                .into_any_element(),
        )
    }

    fn diff_header(
        &self,
        overlay: &DiffOverlayState,
        chrome: DocumentChrome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (branch, files, untracked, added, removed) = match &overlay.load {
            DiffLoad::Ready(s) => {
                let stats = s.stats();
                let (a, r) = stats.totals;
                (s.branch.clone(), s.files.len(), stats.untracked_count, a, r)
            }
            _ => (String::new(), 0, 0, 0, 0),
        };
        // See `render_editor_header`: the traffic-light inset belongs to a
        // header that starts at the window's left edge, which a column's does
        // not.
        let lead = if self.left_panel_open(cx) || chrome.is_dock() {
            crate::ui::app::CONTENT_INSET
        } else {
            crate::ui::app::TITLE_BAR_LEAD
        };
        let mono = SharedString::from(self.font_family.clone());
        let subject = source_subject(&overlay.source, branch);
        let subject_takes_the_slack =
            chrome.is_dock() && !subject.is_rev && subject.label.is_none();
        let menu_app = cx.entity().downgrade();
        let below_chrome = !chrome.is_dock() && self.document_header_below_chrome(window, cx);
        let row = h_flex().id("diff-overlay-header");
        let row = if chrome.header_is_title_strip() && !below_chrome {
            crate::ui::app::title_bar_drag(row, "diff-overlay-header", window, cx)
        } else {
            row
        };
        row.flex_shrink_0()
            .h(px(crate::ui::app::TITLE_BAR_HEIGHT))
            .pl(px(lead))
            .pr(px(crate::ui::app::tile_trailing_inset()))
            .when(
                chrome.renders_own_header() && !self.right_panel_open(cx) && !below_chrome,
                |row| {
                    row.pr(px(crate::ui::app::tile_trailing_inset()
                        + crate::ui::tab_strip::trailing_chrome_w()))
                },
            )
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .items_center()
                    .gap_2()
                    .child(
                        gpui::svg()
                            .path(subject.icon)
                            .flex_shrink_0()
                            .size(px(13.))
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(if subject.is_rev {
                        // A revision is an identifier, not a name: it belongs in the
                        // same monospace the patch below it is set in.
                        div()
                            .flex_shrink_0()
                            .text_size(px(13.))
                            .font_family(self.font_family.clone())
                            .child(subject.text)
                            .into_any_element()
                    } else {
                        // Docked, this is the name that gives: the header has a
                        // column's width rather than a window's, and a branch name that
                        // refused to yield any of it pushed the view toggle and the
                        // close tile off the end. It takes the slack the spacer below
                        // would otherwise have — the same trade the label branch makes,
                        // and for the same reason two `flex_1` siblings would split the
                        // line and truncate the name with empty space beside it.
                        div()
                            .when(subject_takes_the_slack, |d| d.flex_1().min_w_0().truncate())
                            .when(!subject_takes_the_slack, |d| d.flex_shrink_0())
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(subject.text)
                            .into_any_element()
                    })
                    .when_some(subject.chip, |bar, text| {
                        bar.child(info_chip(
                            text,
                            cx.theme().accent.opacity(0.16),
                            cx.theme().foreground,
                            &mono,
                        ))
                    })
                    // The subject takes the slack the spacer below would otherwise
                    // have, which is why that one is skipped when a label is present:
                    // two `flex_1` siblings split the line in half and the subject
                    // would truncate with empty space beside it.
                    .when_some(subject.label.as_ref(), |bar, _| {
                        bar.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(t(L10nKey::ScmViewCommitDetails)),
                        )
                    })
                    .when_some(focused_name(overlay), |bar, name| {
                        bar.child(
                            div().occlude().flex_1().min_w_0().child(
                                h_flex()
                                    .id("diff-overlay-unfocus")
                                    .w_full()
                                    .min_w_0()
                                    .items_center()
                                    .gap_1()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(cx.theme().list_hover))
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        let active = this.active;
                                        if let Some(overlay) = this
                                            .tabs
                                            .get_mut(active)
                                            .and_then(|t| t.diff_overlay.as_mut())
                                        {
                                            overlay.focus = None;
                                            *overlay.anchor.borrow_mut() = None;
                                            cx.notify();
                                        }
                                    }))
                                    .child(
                                        Icon::new(IconName::ChevronLeft)
                                            .small()
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .text_xs()
                                            .font_family(self.font_family.clone())
                                            .child(name),
                                    ),
                            ),
                        )
                    })
                    .when(
                        matches!(overlay.load, DiffLoad::Ready(_))
                            && overlay.focus.is_none()
                            && subject.label.is_none(),
                        |bar| {
                            let mut summary = t_plural(L10nKey::DiffChangedFiles, files, &[]);
                            if untracked > 0 {
                                summary.push_str(&t_plural(
                                    L10nKey::DiffUntrackedCount,
                                    untracked,
                                    &[],
                                ));
                            }
                            // The file count is the first thing a column drops: the
                            // same number is one line down, at the top of the list.
                            // The totals stay — they have no second home.
                            bar.when(!chrome.is_dock(), |bar| {
                                bar.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(summary),
                                )
                            })
                            .when(added > 0, |bar| {
                                bar.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().success)
                                        .child(format!("+{added}")),
                                )
                            })
                            .when(removed > 0, |bar| {
                                bar.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().danger)
                                        .child(format!("−{removed}")),
                                )
                            })
                        },
                    )
                    .when(
                        overlay.loading && matches!(overlay.load, DiffLoad::Ready(_)),
                        |bar| {
                            bar.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t(L10nKey::Refreshing)),
                            )
                        },
                    )
                    .when(subject.label.is_none() && !subject_takes_the_slack, |bar| {
                        bar.child(div().flex_1())
                    }),
            )
            .child(self.diff_document_actions(overlay, cx))
            .child(
                div().occlude().flex_shrink_0().child(
                    crate::ui::tab_strip::chrome_tile_sized(
                        Button::new("diff-overlay-close").icon(Icon::new(IconName::Close)),
                        crate::ui::app::TILE_SIZE,
                        crate::ui::app::TILE_GLYPH_LINE,
                        false,
                        cx,
                    )
                    .rounded_lg()
                    .tooltip(t(L10nKey::DiffCloseTooltip))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_diff_overlay(window, cx);
                    })),
                ),
            )
            .context_menu(move |menu, _window, cx| {
                Tty7App::document_header_menu(menu, &menu_app, cx)
            })
    }

    pub(crate) fn diff_document_actions(
        &self,
        overlay: &DiffOverlayState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .flex_none()
            .items_center()
            .gap_2()
            .when(
                !self.has_commit_document() || commit_has_text_diff(overlay),
                |bar| bar.child(self.diff_view_button(cx)),
            )
            .when_some(diff_context_path(overlay), |bar, path| {
                bar.child(self.diff_context_button(&path, cx))
                    .child(self.diff_change_button(false, cx))
                    .child(self.diff_change_button(true, cx))
            })
            .when(!self.has_commit_document(), |bar| {
                bar.child(self.document_fill_button(cx))
            })
            .into_any_element()
    }

    /// Dispatch the byte read behind an untracked file's preview, at most
    /// once per (path, snapshot). Runs from `render`, so the guards are the
    /// point: `preview` says the answer is in hand, `preview_loading` says it
    /// is on the way.
    fn spawn_untracked_preview_if_needed(&mut self, cx: &mut Context<Self>) {
        let want = {
            let overlay = self
                .tabs
                .get(self.active)
                .and_then(|t| t.diff_overlay.as_ref());
            match overlay {
                Some(o) => match &o.load {
                    DiffLoad::Ready(snap) => {
                        untracked_focus(snap, o.focus.as_deref()).and_then(|path| {
                            let seen = o.preview.as_ref().is_some_and(|(held, _)| held == path)
                                || o.preview_loading.as_deref() == Some(path);
                            (!seen).then(|| (o.host_id, snap.root.clone(), path.to_string()))
                        })
                    }
                    _ => None,
                },
                None => None,
            }
        };
        let Some((host_id, root, path)) = want else {
            return;
        };
        let Some(host) = crate::ui::host_registry::HostRegistry::lookup(cx, host_id) else {
            return;
        };
        let active = self.active;
        if let Some(o) = self
            .tabs
            .get_mut(active)
            .and_then(|t| t.diff_overlay.as_mut())
        {
            o.preview_loading = Some(path.clone());
        }
        let read_path = host.join(&root, &path);
        let landing_host = host.clone();
        let native_pane =
            crate::ui::host_registry::HostRegistry::pane_lifetime(cx, host_id).is_some();
        let key_path = path.clone();
        crate::ui::host_ops::HostOps::run(
            host,
            cx,
            move |h| {
                h.read_file(&read_path, MAX_PREVIEW_BYTES)
                    .ok()
                    .map(|bytes| {
                        Arc::new(git_diff::synthesize_added(
                            &path,
                            &bytes,
                            &git_diff::DiffBudget::SINGLE_FILE,
                        ))
                    })
            },
            move |this, file, cx| {
                if native_pane && !landing_host.is_connected() {
                    this.retire_disconnected_pane_diffs(cx);
                    return;
                }
                let active = this.active;
                let Some(o) = this
                    .tabs
                    .get_mut(active)
                    .and_then(|t| t.diff_overlay.as_mut())
                    .filter(|o| o.host_id == host_id)
                else {
                    return;
                };
                if o.preview_loading.as_deref() == Some(key_path.as_str()) {
                    o.preview_loading = None;
                }
                o.preview = Some((key_path.clone(), file));
                cx.notify();
            },
        );
    }

    /// The one synthesized card, in the same scroll shell the file list uses.
    fn diff_preview_card(
        &self,
        file: &FileDiff,
        scroll: &gpui::ScrollHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = view_mode(cx);
        let list = v_flex()
            .gap_3()
            .p_4()
            .w_full()
            // `usize::MAX` keeps the element ids clear of the real list's.
            .child(self.diff_file_card(usize::MAX, file, true, mode, window, cx));
        crate::ui::scrollbar::with_vertical_scrollbar(
            "diff-overlay-scrollbar",
            div()
                .id("diff-overlay-scroll")
                .debug_selector(|| "diff-scroll".into())
                .flex_1()
                .min_h_0()
                .w_full()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .child(list),
            scroll,
        )
    }

    fn diff_message(&self, text: &'static str, cx: &Context<Self>) -> AnyElement {
        let palette = diff_palette(cx);
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_sm()
            .text_color(palette.muted)
            .child(text)
            .into_any_element()
    }

    fn diff_file_list(
        &self,
        snap: &DiffSnapshot,
        expanded: &HashMap<String, bool>,
        focused: Option<usize>,
        scroll: &gpui::ScrollHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let stats = snap.stats();
        let palette = diff_palette(cx);
        let mode = view_mode(cx);
        let oversized = focused.is_none() && stats.oversized;
        let mut list = v_flex().gap_3().p_4().w_full();
        if oversized {
            list = list.child(self.diff_oversized_notice(snap, &stats, cx));
        }
        let shown = snap.files.len().min(MAX_RENDERED_FILES);
        for (idx, file) in snap.files.iter().enumerate() {
            if focused.is_some_and(|f| f != idx) {
                continue;
            }
            if focused.is_none() && idx >= shown {
                break;
            }
            let file = self
                .tabs
                .get(self.active)
                .and_then(|t| t.diff_overlay.as_ref())
                .and_then(|o| o.file_views.get(&file.path))
                .and_then(|v| v.shown.as_deref())
                .unwrap_or(file);
            let is_expanded = if focused == Some(idx) {
                expanded.get(&file.path).copied().unwrap_or(true)
            } else {
                file_expanded(file, expanded, oversized)
            };
            list = list.child(self.diff_file_card(idx, file, is_expanded, mode, window, cx));
        }
        if focused.is_none() && snap.files.len() > shown {
            let rest = snap.files.len() - shown;
            list = list.child(
                div()
                    .w_full()
                    .px_2p5()
                    .py_1p5()
                    .text_xs()
                    .text_color(palette.muted)
                    .child(t_plural(L10nKey::DiffMoreFiles, rest, &[])),
            );
        }
        if focused.is_none() && !snap.untracked.is_empty() {
            list = list.child(self.diff_untracked_section(snap, cx));
        }
        // A whole working tree can scroll past here with nothing to say how
        // far it runs or where in it you are — the one long document in the
        // app without the bar every other scroll area has.
        crate::ui::scrollbar::with_vertical_scrollbar(
            "diff-overlay-scrollbar",
            div()
                .id("diff-overlay-scroll")
                .debug_selector(|| "diff-scroll".into())
                .flex_1()
                .min_h_0()
                .w_full()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .child(list),
            scroll,
        )
    }

    fn diff_oversized_notice(
        &self,
        snap: &DiffSnapshot,
        stats: &DiffStats,
        cx: &Context<Self>,
    ) -> AnyElement {
        let palette = diff_palette(cx);
        let text = t_fmt(
            L10nKey::DiffOversizedNotice,
            &[("summary", &oversized_summary(snap, stats))],
        );
        div()
            .w_full()
            .px_2p5()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(palette.border)
            .bg(palette.header)
            .text_xs()
            .text_color(palette.muted)
            .child(text)
            .into_any_element()
    }

    fn diff_file_card(
        &self,
        idx: usize,
        file: &FileDiff,
        expanded: bool,
        mode: DiffViewMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let palette = diff_palette(cx);
        let expandable =
            !file.binary && (!file.hunks.is_empty() || file.truncated == Some(Truncation::Budget));
        let deco = deco_status(file.status);
        let glyph = status_glyph(deco);
        let glyph_color = match file.status {
            FileStatus::Added => palette.added,
            FileStatus::Deleted => palette.removed,
            _ => palette.changed,
        };
        let shown_path = match &file.old_path {
            Some(old) => format!("{old} → {}", file.path),
            None => file.path.clone(),
        };

        let has_body = expanded && (!file.hunks.is_empty() || file.truncated.is_some());
        let context_failed = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.diff_overlay.as_ref())
            .and_then(|overlay| overlay.file_views.get(&file.path))
            .is_some_and(|view| view.failed);

        let header_corners = rounding::stack_corners(
            0,
            if has_body { 2 } else { 1 },
            rounding::CARD_RADIUS,
            rounding::HAIRLINE,
        );
        let mut header = h_flex()
            .id(("diff-file-header", idx))
            .debug_selector(move || format!("diff-file-header:{idx}"))
            .w_full()
            .items_center()
            .gap_2()
            .px_2p5()
            .py_1p5()
            .rounded_corners(header_corners)
            .bg(palette.header)
            .when(expandable, |h| {
                let path = file.path.clone();
                h.cursor_pointer()
                    .hover(|s| s.bg(palette.background))
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        let active = this.active;
                        if let Some(overlay) = this
                            .tabs
                            .get_mut(active)
                            .and_then(|t| t.diff_overlay.as_mut())
                        {
                            overlay.expanded.insert(path.clone(), !expanded);
                            cx.notify();
                        }
                    }))
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .small()
                        .text_color(palette.muted),
                    )
            })
            .child(
                div()
                    .flex_shrink_0()
                    .font_family(self.font_family.clone())
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(glyph_color)
                    .child(glyph),
            )
            .child(
                div()
                    .debug_selector(move || format!("diff-file-name:{idx}"))
                    .flex_1()
                    .min_w_0()
                    .child(SelectableLabel {
                        id: format!("diff-file-name-selection:{}", file.path).into(),
                        text: shown_path.into(),
                        font: gpui::Font {
                            family: self.font_family.clone().into(),
                            ..Default::default()
                        },
                        font_size: px(window.rem_size().as_f32() * 0.75),
                    }),
            );
        if file.binary {
            header = header.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(palette.muted)
                    .child(t(L10nKey::Binary)),
            );
        }
        if file.added > 0 {
            header = header.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(palette.added)
                    .child(format!("+{}", file.added)),
            );
        }
        if file.removed > 0 {
            header = header.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(palette.removed)
                    .child(format!("−{}", file.removed)),
            );
        }
        if !file.binary && idx != usize::MAX {
            header = header.child(self.diff_context_button(&file.path, cx));
        }

        let mut card = v_flex()
            .debug_selector(|| {
                if idx == usize::MAX {
                    "diff-preview-card".into()
                } else {
                    format!("diff-card-{idx}")
                }
            })
            .w_full()
            .border_1()
            .border_color(palette.border)
            .bg(palette.background)
            .text_color(palette.foreground)
            .rounded(rounding::CARD_RADIUS)
            .overflow_hidden()
            .child(header);

        if has_body {
            let hunks: Vec<_> = file
                .hunks
                .iter()
                .map(|hunk| {
                    let rows = match mode {
                        DiffViewMode::Split => HunkRows::Split(split_hunk(&hunk.lines)),
                        DiffViewMode::Unified => HunkRows::Unified(unified_rows(&hunk.lines)),
                    };
                    (hunk.header.clone(), Arc::new(rows))
                })
                .collect();
            let closing_row = if file.truncated.is_some() {
                None
            } else {
                hunks
                    .iter()
                    .rposition(|(_, rows)| !rows.is_empty())
                    .map(|h| (h, hunks[h].1.len() - 1))
            };
            let font = gpui::Font {
                family: self.font_family.clone().into(),
                ..Default::default()
            };
            let font_size = window.rem_size().as_f32() * 0.75;
            let sides: &[Option<Side>] = match mode {
                DiffViewMode::Split => &[Some(Side::Old), Some(Side::New)],
                DiffViewMode::Unified => &[None],
            };
            let selection = window.use_keyed_state(
                SharedString::from(format!("diff-selection:{}", file.path)),
                cx,
                |_, cx| DiffSelection::new(cx),
            );
            selection.update(cx, |state, _| {
                state.configure(
                    mode,
                    sides
                        .iter()
                        .map(|side| {
                            file.hunks
                                .iter()
                                .flat_map(|hunk| &hunk.lines)
                                .filter(|line| match side {
                                    Some(Side::Old) => line.kind != LineKind::Added,
                                    Some(Side::New) => line.kind != LineKind::Removed,
                                    None => true,
                                })
                                .map(|line| SharedString::from(line.text.clone()))
                                .collect()
                        })
                        .collect(),
                )
            });
            card = selectable_card(card, &selection, cx);
            let vertical = self
                .tabs
                .get(self.active)
                .and_then(|tab| tab.diff_overlay.as_ref())
                .map(|overlay| overlay.scroll.clone());
            let mut hunk_style = window.text_style();
            hunk_style.font_size = gpui::rems(0.75).into();
            let hunk_height =
                hunk_style.line_height_in_pixels(window.rem_size()) + window.rem_size() * 0.25;
            let view = self
                .tabs
                .get(self.active)
                .and_then(|t| t.diff_overlay.as_ref())
                .and_then(|o| o.file_views.get(&file.path));
            let whole = view.is_some_and(|v| v.whole && v.shown.is_some());
            let mut previous_end = 0;
            let gaps: Vec<_> = file
                .hunks
                .iter()
                .map(|h| {
                    let first = h.lines.iter().find_map(|l| l.new_no).unwrap_or(0);
                    let gap = (!whole && first > previous_end + 1).then(|| {
                        (
                            first.saturating_sub(20).max(previous_end + 1)..first,
                            previous_end + 1..(previous_end + 21).min(first),
                        )
                    });
                    previous_end = h
                        .lines
                        .iter()
                        .filter_map(|l| l.new_no)
                        .last()
                        .unwrap_or(previous_end);
                    gap
                })
                .collect();
            let hunk_control_width = if gaps.iter().any(Option::is_some) {
                DIFF_HUNK_CONTROL_WIDTH
            } else {
                0.
            };
            let hunk_heights: Vec<_> = gaps
                .iter()
                .enumerate()
                .map(|(h, gap)| {
                    if h > 0
                        && gap
                            .as_ref()
                            .is_some_and(|(above, below)| above.start > below.end)
                    {
                        hunk_height * 2.
                    } else {
                        hunk_height
                    }
                })
                .collect();
            let hunk_total = hunk_heights
                .iter()
                .fold(px(0.), |total, height| total + *height);
            let body_height = hunk_total
                + px(
                    hunks.iter().map(|(_, rows)| rows.len()).sum::<usize>() as f32
                        * DIFF_ROW_HEIGHT,
                )
                + px(12.);
            let mut positions = Vec::new();
            let mut y = 0.;
            for (hunk_index, (_, rows)) in hunks.iter().enumerate() {
                y += hunk_heights[hunk_index].as_f32();
                match rows.as_ref() {
                    HunkRows::Split(rows) => {
                        for row in rows {
                            positions.push((
                                (
                                    row.left.as_ref().and_then(|c| c.no),
                                    row.right.as_ref().and_then(|c| c.no),
                                ),
                                y,
                                row.left.as_ref().is_some_and(|c| c.changed)
                                    || row.right.as_ref().is_some_and(|c| c.changed),
                            ));
                            y += DIFF_ROW_HEIGHT;
                        }
                    }
                    HunkRows::Unified(rows) => {
                        for row in rows {
                            positions.push(((row.old, row.new), y, row.kind != LineKind::Context));
                            y += DIFF_ROW_HEIGHT;
                        }
                    }
                }
            }
            let positions = Arc::new(positions);
            let context_path = file.path.clone();
            let geometry = self
                .tabs
                .get(self.active)
                .and_then(|t| t.diff_overlay.as_ref())
                .map(|o| (o.geometry.clone(), o.anchor.clone(), o.scroll.clone()));
            let geometry_path = file.path.clone();
            // Keep each horizontal offset even while its file is offscreen.
            let horizontal: Vec<_> = sides
                .iter()
                .map(|side| {
                    let key = match side {
                        Some(Side::Old) => "diff-old-scroll",
                        Some(Side::New) => "diff-new-scroll",
                        None => "diff-unified-scroll",
                    };
                    let scroll = window
                        .use_keyed_state((key, idx), cx, |_, _| gpui::ScrollHandle::new())
                        .read(cx)
                        .clone();
                    (key, scroll)
                })
                .collect();
            let owner = cx.entity();
            let columns = gpui::canvas(
                move |bounds, window, cx| {
                    if let Some((geometry, anchor, scroll)) = &geometry {
                        geometry.borrow_mut().insert(
                            geometry_path.clone(),
                            DiffGeometry {
                                content_top: bounds.top().as_f32() - scroll.offset().y.as_f32(),
                                rows: positions.clone(),
                            },
                        );
                        let mut pending = anchor.borrow_mut();
                        if let Some(a) = pending.as_ref().filter(|a| a.path == geometry_path) {
                            if let Some((_, y, _)) = anchor_row(&positions, a.line) {
                                let delta = a.screen_y - bounds.top().as_f32() - y;
                                if delta.abs() > 0.5 {
                                    let scroll = scroll.clone();
                                    let handle = window.window_handle();
                                    // Defer until layout has installed the new scroll
                                    // range. This also works without animation frames.
                                    cx.defer(move |cx| {
                                        let _ = handle.update(cx, |_, window, _| {
                                            let mut offset = scroll.offset();
                                            offset.y += px(delta);
                                            scroll.set_offset(offset);
                                            window.refresh();
                                        });
                                    });
                                    window.refresh();
                                }
                            }
                            *pending = None;
                        }
                    }
                    let visible = bounds.intersect(&window.content_mask().bounds);
                    if visible.size.width <= px(0.) || visible.size.height <= px(0.) {
                        selection.update(cx, |state, _| state.cancel_drag());
                        return None;
                    }
                    let mut columns = owner.update(cx, |app, cx| {
                        let palette = diff_palette(cx);
                        let mut columns = h_flex()
                            .debug_selector(|| "diff-columns".into())
                            .w_full()
                            .items_stretch();
                        for (column, &side) in sides.iter().enumerate() {
                            let mut row_index = 0;
                            let mut body = v_flex().w_full().flex_shrink_0();
                            let measure = |text: &str| {
                                crate::ui::tab_strip::measure_text(
                                    window.text_system(),
                                    &font,
                                    font_size,
                                    text,
                                )
                            };
                            let divider = if sides.len() == 2 {
                                DIFF_DIVIDER_WIDTH
                            } else {
                                0.
                            };
                            let column_width =
                                (bounds.size.width.as_f32() - divider) / sides.len() as f32;
                            let mut width = 0_f32;
                            for (header, rows) in &hunks {
                                width = width.max(measure(header) + 16. + hunk_control_width);
                                match rows.as_ref() {
                                    HunkRows::Split(rows) => {
                                        let side = side.expect("split column");
                                        for cell in rows.iter().filter_map(|row| row.cell(side)) {
                                            width = width.max(
                                                measure(&split_text(cell, side))
                                                    + SPLIT_GUTTER_WIDTH
                                                    + DIFF_LINE_END_PADDING,
                                            );
                                        }
                                    }
                                    HunkRows::Unified(rows) => {
                                        for row in rows {
                                            width = width.max(
                                                measure(&row.text)
                                                    + UNIFIED_GUTTER_WIDTH * 2.
                                                    + DIFF_DIVIDER_WIDTH
                                                    + DIFF_MARKER_WIDTH
                                                    + DIFF_LINE_END_PADDING,
                                            );
                                        }
                                    }
                                }
                            }
                            for (h, (header, rows)) in hunks.iter().enumerate() {
                                let mut hunk_header = h_flex()
                                    .debug_selector(move || {
                                        format!("diff-hunk-header:{column}:{h}")
                                    })
                                    .w_full()
                                    .h(hunk_heights[h])
                                    .flex_shrink_0()
                                    .bg(palette.hunk_background)
                                    .text_xs()
                                    .font_family(app.font_family.clone())
                                    .text_color(palette.hunk)
                                    .whitespace_nowrap();
                                if column == 0 && hunk_control_width > 0. {
                                    let mut gutter = v_flex()
                                        .w(px(DIFF_HUNK_CONTROL_WIDTH))
                                        .h_full()
                                        .flex_shrink_0()
                                        .items_stretch()
                                        .justify_center();
                                    if let Some((above, below)) = gaps[h].clone() {
                                        gutter = gutter.bg(palette.hunk_controls);
                                        if h == 0 {
                                            gutter = gutter.child(app.diff_more_button(
                                                &context_path,
                                                above,
                                                Some(true),
                                                cx,
                                            ));
                                        } else if above.start <= below.end {
                                            gutter = gutter.child(app.diff_more_button(
                                                &context_path,
                                                below.start..above.end,
                                                None,
                                                cx,
                                            ));
                                        } else {
                                            gutter = gutter
                                                .child(app.diff_more_button(
                                                    &context_path,
                                                    above,
                                                    Some(true),
                                                    cx,
                                                ))
                                                .child(app.diff_more_button(
                                                    &context_path,
                                                    below,
                                                    Some(false),
                                                    cx,
                                                ));
                                        }
                                    }
                                    hunk_header = hunk_header.child(gutter);
                                }
                                hunk_header = hunk_header.child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .px_2()
                                        .py_0p5()
                                        .child(header.clone()),
                                );
                                body = body.child(hunk_header);
                                let row_offset = row_index;
                                match rows.as_ref() {
                                    HunkRows::Split(rows) => {
                                        let side = side.expect("split column");
                                        row_index += rows
                                            .iter()
                                            .filter(|row| row.cell(side).is_some())
                                            .count();
                                    }
                                    HunkRows::Unified(rows) => {
                                        row_index += rows.len();
                                    }
                                }
                                let owner = cx.entity();
                                let rows = Arc::clone(rows);
                                let selection = selection.clone();
                                let font = font.clone();
                                body = body.child(visible_diff_rows(
                                    rows.len(),
                                    width,
                                    move |range, _, cx| {
                                        owner.update(cx, |app, cx| {
                                            let mut body = v_flex().w_full();
                                            let mut row_index = row_offset;
                                            match rows.as_ref() {
                                                HunkRows::Split(rows) => {
                                                    let side = side.expect("split column");
                                                    row_index += rows[..range.start]
                                                        .iter()
                                                        .filter(|row| row.cell(side).is_some())
                                                        .count();
                                                    for r in range {
                                                        let cell = rows[r].cell(side);
                                                        let radius = if closing_row == Some((h, r))
                                                        {
                                                            rounding::inner_radius(
                                                                rounding::CARD_RADIUS,
                                                                rounding::HAIRLINE,
                                                            )
                                                        } else {
                                                            px(0.)
                                                        };
                                                        body = body.child(
                                                            app.diff_split_cell_selected(
                                                                cell,
                                                                side,
                                                                radius,
                                                                cell.map(|_| SelectionLine {
                                                                    state: selection.clone(),
                                                                    column,
                                                                    row: row_index,
                                                                    font: font.clone(),
                                                                    font_size: px(font_size),
                                                                }),
                                                                cx,
                                                            )
                                                            .flex_none()
                                                            .w_full()
                                                            .h(px(DIFF_ROW_HEIGHT))
                                                            .text_xs()
                                                            .font_family(app.font_family.clone()),
                                                        );
                                                        if cell.is_some() {
                                                            row_index += 1;
                                                        }
                                                    }
                                                }
                                                HunkRows::Unified(rows) => {
                                                    for r in range {
                                                        body = body.child(
                                                            app.diff_unified_row_selected(
                                                                &rows[r],
                                                                closing_row == Some((h, r)),
                                                                Some(SelectionLine {
                                                                    state: selection.clone(),
                                                                    column,
                                                                    row: row_offset + r,
                                                                    font: font.clone(),
                                                                    font_size: px(font_size),
                                                                }),
                                                                cx,
                                                            )
                                                            .flex_shrink_0(),
                                                        );
                                                    }
                                                }
                                            }
                                            body.into_any_element()
                                        })
                                    },
                                ));
                            }
                            let (key, scroll) = &horizontal[column];
                            let key = *key;
                            let scroll = scroll.clone();
                            if side == Some(Side::New) {
                                columns = columns.child(
                                    div()
                                        .flex_shrink_0()
                                        .w(px(DIFF_DIVIDER_WIDTH))
                                        .bg(palette.border),
                                );
                            }
                            columns = columns.child(
                    div()
                        .relative()
                        .flex_1()
                        .min_w_0()
                        .pb(px(12.))
                        .child(selectable_viewport(
                            div()
                                .id((key, idx))
                                .debug_selector(move || key.into())
                                .relative()
                                .w_full()
                                // Keep the wheel hitbox on the viewport as its content moves.
                                .overflow_hidden()
                                .track_scroll(&scroll)
                                .on_scroll_wheel(cx.listener({
                                    let scroll = scroll.clone();
                                    move |_, event: &gpui::ScrollWheelEvent, window, cx| {
                                        let delta = event.delta.pixel_delta(window.line_height());
                                        // Vertical gestures continue to the file list.
                                        if delta.x.abs() > delta.y.abs() {
                                            let mut offset = scroll.offset();
                                            offset.x += delta.x;
                                            scroll.set_offset(offset);
                                            cx.notify();
                                            cx.stop_propagation();
                                        }
                                    }
                                }))
                                // A definite width also fills short hunk headers when
                                // another row makes the column horizontally scrollable.
                                .child(body.w(px(width.ceil().max(column_width)))),
                            &selection,
                            column,
                            scroll.clone(),
                            vertical.clone(),
                            cx,
                        ))
                        .child(
                            div()
                                .debug_selector(|| "diff-horizontal-scrollbar".into())
                                .absolute()
                                .bottom_0()
                                .w_full()
                                .h(px(12.))
                                .child(
                                    Scrollbar::horizontal(&scroll)
                                        .id((key, idx))
                                        .scrollbar_show(ScrollbarShow::Always),
                                ),
                        ),
                );
                        }
                        columns.into_any_element()
                    });
                    columns.layout_as_root(
                        gpui::size(
                            gpui::AvailableSpace::Definite(bounds.size.width),
                            gpui::AvailableSpace::Definite(bounds.size.height),
                        ),
                        window,
                        cx,
                    );
                    columns.prepaint_at(bounds.origin, window, cx);
                    Some(columns)
                },
                |_, columns, window, cx| {
                    if let Some(mut columns) = columns {
                        columns.paint(window, cx);
                    }
                },
            )
            .w_full()
            .h(body_height)
            .flex_shrink_0();
            card = card.child(columns);
            if let Some(reason) = file.truncated {
                let note = match reason {
                    Truncation::PerFile => t_fmt(
                        L10nKey::DiffTruncatedPerFile,
                        &[(
                            "limit",
                            &if view.is_some_and(|v| v.shown.is_some()) {
                                git_diff::DiffBudget::SINGLE_FILE.max_lines_per_file
                            } else {
                                git_diff::MAX_LINES_PER_FILE
                            }
                            .to_string(),
                        )],
                    ),
                    Truncation::Budget => t(L10nKey::DiffTruncatedBudget).to_string(),
                };
                card = card.child(
                    div()
                        .w_full()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(palette.muted)
                        .child(note),
                );
            }
        }
        if context_failed {
            card = card.child(
                div()
                    .debug_selector(|| format!("diff-context-error:{idx}"))
                    .px_2p5()
                    .py_1p5()
                    .text_xs()
                    .text_color(palette.muted)
                    .child(t(L10nKey::DiffReadFailed)),
            );
        }
        card
    }

    /// Show the current arrangement and name the destination in the tooltip.
    /// Switching reuses the existing preference without replacing the overlay.
    fn diff_view_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let split = view_mode(cx) == DiffViewMode::Split;
        let icon = div()
            .size(px(crate::ui::app::TILE_GLYPH_LINE))
            .flex()
            .when(split, |icon| icon.flex_row())
            .when(!split, |icon| icon.flex_col())
            .gap(px(2.))
            .p(px(2.))
            .border_1()
            .border_color(cx.theme().muted_foreground)
            .rounded_sm()
            .children(
                [cx.theme().danger, cx.theme().success]
                    .map(|color| div().flex_1().min_w_0().min_h_0().bg(color.opacity(0.65))),
            );
        div()
            .debug_selector(|| "diff-view-toggle".into())
            .occlude()
            .flex_shrink_0()
            .child(
                crate::ui::tab_strip::chrome_tile_sized(
                    Button::new("diff-view-toggle").child(icon),
                    crate::ui::app::TILE_SIZE,
                    crate::ui::app::TILE_GLYPH_LINE,
                    false,
                    cx,
                )
                .rounded_lg()
                .tooltip(t(if split {
                    L10nKey::DiffViewUnifiedTooltip
                } else {
                    L10nKey::DiffViewSplitTooltip
                }))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(o) = this
                        .tabs
                        .get(this.active)
                        .and_then(|t| t.diff_overlay.as_ref())
                    {
                        if let Some(path) = &o.focus {
                            o.remember_diff_position(path);
                        }
                    }
                    this.toggle_diff_view_mode(cx)
                })),
            )
            .into_any_element()
    }

    #[cfg(test)]
    fn diff_split_cell(
        &self,
        cell: Option<&SplitCell>,
        side: Side,
        outer_radius: Pixels,
        cx: &Context<Self>,
    ) -> gpui::Div {
        self.diff_split_cell_selected(cell, side, outer_radius, None, cx)
    }

    fn diff_split_cell_selected(
        &self,
        cell: Option<&SplitCell>,
        side: Side,
        outer_radius: Pixels,
        selection: Option<SelectionLine>,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let palette = diff_palette(cx);
        let base = h_flex()
            .debug_selector(|| match side {
                Side::Old => "diff-old-cell".into(),
                Side::New => "diff-new-cell".into(),
            })
            .flex_1()
            .min_w_0()
            .h_full()
            .items_center();
        let base = match side {
            Side::Old => base.rounded_bl(outer_radius),
            Side::New => base.rounded_br(outer_radius),
        };
        let Some(cell) = cell else {
            return base.bg(palette.header);
        };
        let (foreground, background) = match (cell.changed, side) {
            (true, Side::Old) => (palette.removed, palette.removed_background),
            (true, Side::New) => (palette.added, palette.added_background),
            (false, _) => (palette.foreground, palette.background),
        };
        let text = SharedString::from(split_text(cell, side));
        base.bg(background)
            .text_color(foreground)
            .child(
                h_flex()
                    .flex_shrink_0()
                    .w(px(SPLIT_GUTTER_WIDTH))
                    .justify_end()
                    .pr_1p5()
                    .text_color(palette.muted)
                    .child(cell.no.map(|n| n.to_string()).unwrap_or_default()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .debug_selector(move || match side {
                        Side::Old => "diff-old-text".into(),
                        Side::New => "diff-new-text".into(),
                    })
                    .when_some(selection, |el, line| {
                        line.attach(el, text.clone(), text.len() - cell.text.len())
                    })
                    .child(text),
            )
    }

    /// One line of the unified view.
    ///
    /// Every measurement it shares with [`Self::diff_split_cell`] is shared on
    /// purpose — the same 19px row, the same `text_xs` in the same family, and
    /// the same GitHub colors for additions, deletions and context. The two
    /// views are one diff seen twice; a different green would read as a
    /// different thing.
    ///
    /// What differs is forced by the shape. The line numbers get 34px a side
    /// rather than 42 (there are two gutters here in front of one column of
    /// text, not one in front of each), and the `+`/`−` gets a column of its
    /// own rather than riding in the text: with three kinds of line stacked in
    /// one column, an inlined marker would leave the context lines' code
    /// starting two characters left of everything else.
    #[cfg(test)]
    fn diff_unified_row(
        &self,
        row: &UnifiedRow,
        closes_card: bool,
        cx: &Context<Self>,
    ) -> gpui::Div {
        self.diff_unified_row_selected(row, closes_card, None, cx)
    }

    fn diff_unified_row_selected(
        &self,
        row: &UnifiedRow,
        closes_card: bool,
        selection: Option<SelectionLine>,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let palette = diff_palette(cx);
        let radius = if closes_card {
            rounding::inner_radius(rounding::CARD_RADIUS, rounding::HAIRLINE)
        } else {
            px(0.)
        };
        let (foreground, background) = match row.kind {
            LineKind::Added => (palette.added, palette.added_background),
            LineKind::Removed => (palette.removed, palette.removed_background),
            LineKind::Context => (palette.foreground, palette.background),
        };
        let gutter = |no: Option<u32>| {
            h_flex()
                .flex_shrink_0()
                .w(px(UNIFIED_GUTTER_WIDTH))
                .justify_end()
                .pr_1p5()
                .text_color(palette.muted)
                .child(no.map(|n| n.to_string()).unwrap_or_default())
        };
        h_flex()
            .debug_selector(|| "diff-row".into())
            .w_full()
            .h(px(19.))
            .items_center()
            .text_xs()
            .font_family(self.font_family.clone())
            .rounded_bl(radius)
            .rounded_br(radius)
            .bg(background)
            .text_color(foreground)
            .child(gutter(row.old))
            .child(gutter(row.new))
            // The split view's centre rule, in the one place it still means the
            // same thing: everything left of it is a number, everything right
            // of it is the file.
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(DIFF_DIVIDER_WIDTH))
                    .h_full()
                    .bg(palette.border),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(DIFF_MARKER_WIDTH))
                    .text_center()
                    .text_color(foreground)
                    .child(unified_marker(row.kind)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .debug_selector(|| "diff-unified-text".into())
                    .when_some(selection, |el, line| {
                        line.attach(el, row.text.clone().into(), 0)
                    })
                    .child(row.text.clone()),
            )
    }

    fn diff_untracked_section(&self, snap: &DiffSnapshot, cx: &Context<Self>) -> AnyElement {
        let palette = diff_palette(cx);
        let total = snap.untracked_count();
        let untracked = &snap.untracked[..snap.untracked.len().min(MAX_RENDERED_FILES)];
        let header_corners = rounding::stack_corners(
            0,
            if total == 0 { 1 } else { 2 },
            rounding::CARD_RADIUS,
            rounding::HAIRLINE,
        );
        let mut section = v_flex()
            .w_full()
            .border_1()
            .border_color(palette.border)
            .bg(palette.background)
            .text_color(palette.foreground)
            .rounded(rounding::CARD_RADIUS)
            .overflow_hidden()
            .child(
                div()
                    .w_full()
                    .px_2p5()
                    .py_1p5()
                    .rounded_corners(header_corners)
                    .bg(palette.header)
                    .text_xs()
                    .text_color(palette.muted)
                    .child(t_plural(L10nKey::DiffUntrackedHeader, total, &[])),
            );
        for (i, path) in untracked.iter().enumerate() {
            // An untracked file has no patch in the snapshot, so it cannot be
            // expanded in place the way the cards above it are — its contents
            // are read one file at a time and shown on their own. The row
            // asks for that read, which until now only the Source Control
            // panel could: in the overlay these rows were the only files in a
            // list of files that did nothing when clicked.
            let for_focus = path.clone();
            section = section.child(
                h_flex()
                    .id(("diff-untracked", i))
                    .w_full()
                    .items_center()
                    .gap_2()
                    .px_2p5()
                    .py_1()
                    .text_xs()
                    .font_family(self.font_family.clone())
                    .cursor_pointer()
                    .hover(|s| s.bg(palette.header))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let Some((host, cwd, source)) = this
                            .tabs
                            .get(this.active)
                            .and_then(|t| t.diff_overlay.as_ref())
                            .map(|o| (o.host_id, o.cwd.clone(), o.source.clone()))
                        else {
                            return;
                        };
                        this.open_diff_overlay(
                            host,
                            cwd,
                            source,
                            Some(for_focus.clone()),
                            window,
                            cx,
                        );
                    }))
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_weight(FontWeight::BOLD)
                            .text_color(palette.added)
                            .child(status_glyph(DecoStatus::Untracked)),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(path.clone())),
            );
        }
        if total > untracked.len() {
            let rest = total - untracked.len();
            section = section.child(
                div()
                    .w_full()
                    .px_2p5()
                    .py_1()
                    .text_xs()
                    .text_color(palette.muted)
                    .child(t_plural(L10nKey::DiffMoreUntracked, rest, &[])),
            );
        }
        section.into_any_element()
    }
}

/// Which layout the overlay draws. One setting for the window, not one per
/// overlay: VS Code's `diffEditor.renderSideBySide` is global for the same
/// reason — re-picking on every open is a chore, not a choice.
fn view_mode(cx: &gpui::App) -> DiffViewMode {
    cx.try_global::<Config>()
        .map(|cfg| cfg.diff_view)
        .unwrap_or_default()
}

/// The change column. `−` is U+2212, matching the split view: the ASCII hyphen
/// is narrower than `+` and the two columns would not line up.
fn unified_marker(kind: LineKind) -> &'static str {
    match kind {
        LineKind::Added => "+",
        LineKind::Removed => "−",
        LineKind::Context => "",
    }
}

fn split_text(cell: &SplitCell, side: Side) -> String {
    let marker = match (cell.changed, side) {
        (true, Side::Old) => "−",
        (true, Side::New) => "+",
        (false, _) => " ",
    };
    format!("{marker} {}", cell.text)
}

/// The git status decoration shared across the app.
///
/// `Copied` and `TypeChanged` have no decoration of their own — porcelain v2's
/// index folds them the same way — so they take the nearest one rather than
/// inventing a `C` and a `T` that appear in the overlay and nowhere else.
pub(crate) fn deco_status(status: FileStatus) -> DecoStatus {
    match status {
        FileStatus::Added => DecoStatus::Added,
        FileStatus::Modified => DecoStatus::Modified,
        FileStatus::Deleted => DecoStatus::Deleted,
        FileStatus::Renamed | FileStatus::Copied => DecoStatus::Renamed,
        FileStatus::TypeChanged => DecoStatus::Modified,
        FileStatus::Unmerged => DecoStatus::Conflict,
    }
}

/// The current epoch for a repository, or 0 where nothing has ever bumped one.
/// Zero is the same value a never-touched repository reports, so an overlay
/// that reads it before the global exists simply never looks stale.
fn scm_epoch(cx: &gpui::App, host: crate::ui::host_ops::HostId, root: &Path) -> u64 {
    cx.try_global::<crate::terminal::git_data::ScmData>()
        .map(|data| data.epoch(host, root))
        .unwrap_or(0)
}

/// What the header calls the patch it is showing.
struct SourceSubject {
    icon: &'static str,
    text: String,
    /// Set only where the branch name alone would be ambiguous.
    chip: Option<&'static str>,
    is_rev: bool,
    /// What the commit is *about*, where whoever opened it knew. An object id
    /// is an address, not a name, and a header with nothing but eight hex
    /// digits leaves the reader to remember which commit that was.
    label: Option<CommitLabel>,
}

fn source_subject(source: &DiffSource, branch: String) -> SourceSubject {
    let branch_of = |chip| SourceSubject {
        icon: "icons/git-branch.svg",
        text: branch.clone(),
        chip,
        is_rev: false,
        label: None,
    };
    match source {
        // Worktree and Head are both "the branch, right now"; the header for
        // them is what it has always been.
        DiffSource::Worktree | DiffSource::Head => branch_of(None),
        // Staged is the branch too, but a patch that does not match the files
        // on disk — without the chip it is indistinguishable from the above.
        DiffSource::Staged => branch_of(Some(t(L10nKey::ScmChipStaged))),
        DiffSource::Commit { rev, label } => SourceSubject {
            icon: "icons/git-commit.svg",
            text: short_rev(rev),
            chip: None,
            is_rev: true,
            // An empty subject is no more use than no label at all, and a
            // `Default::default()` that leaked through would render as one.
            label: label.clone().filter(|l| !l.subject.is_empty()),
        },
        DiffSource::Range { base, head } => SourceSubject {
            icon: "icons/git-commit.svg",
            text: format!("{}…{}", short_rev(base), short_rev(head)),
            chip: None,
            is_rev: true,
            label: None,
        },
    }
}

/// `Ada · 2h`, the byline under a commit's subject.
///
/// One string rather than two elements: the separator has to disappear along
/// with whichever half is missing, and a `when_some` chain around a middle dot
/// says less than this does.
fn label_byline(label: &CommitLabel, now: i64) -> String {
    let when = (label.at > 0).then(|| relative_time(now, label.at));
    match (label.author.trim(), when) {
        ("", Some(when)) => when,
        (author, Some(when)) => format!("{author} · {when}"),
        (author, None) => author.to_string(),
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Object ids get cut to eight characters; anything else is already a name a
/// person chose, and cutting `origin/main` in half would only hide which it is.
fn short_rev(rev: &str) -> String {
    let is_oid = rev.len() >= 40 && rev.chars().all(|c| c.is_ascii_hexdigit());
    match is_oid {
        true => rev[..8].to_string(),
        false => rev.to_string(),
    }
}

fn focused_file(snap: &DiffSnapshot, overlay: &DiffOverlayState) -> Option<usize> {
    let path = overlay.focus.as_deref()?;
    snap.files.iter().position(|f| f.path == path)
}

/// The focused path, when it is an *untracked* file — one the snapshot lists
/// by name but holds no patch for. A path that is both (staged half tracked,
/// say) prefers the real patch.
fn untracked_focus<'a>(snap: &DiffSnapshot, focus: Option<&'a str>) -> Option<&'a str> {
    let path = focus?;
    if snap.files.iter().any(|f| f.path == path) {
        return None;
    }
    snap.untracked.iter().any(|u| u == path).then_some(path)
}

/// The file the overlay is focused on, for the header's way back to the list.
///
/// An untracked file is focused like any other but has no entry in `files` —
/// its card is synthesized from the file's own bytes — so reading only `files`
/// left the one view with no way out of it: the breadcrumb never drew, and the
/// list was reachable again only by closing the overlay and reopening it.
fn focused_name(overlay: &DiffOverlayState) -> Option<String> {
    let DiffLoad::Ready(snap) = &overlay.load else {
        return None;
    };
    if let Some(idx) = focused_file(snap, overlay) {
        return Some(snap.files[idx].path.clone());
    }
    untracked_focus(snap, overlay.focus.as_deref()).map(str::to_string)
}

fn diff_context_path(overlay: &DiffOverlayState) -> Option<String> {
    focused_name(overlay).filter(|path| {
        matches!(&overlay.load, DiffLoad::Ready(s) if s.files.iter().any(|f| f.path == *path && !f.binary))
            && !overlay.file_views.get(path).and_then(|v| v.full.as_ref()).is_some_and(|f| f.binary)
    })
}

pub(crate) fn commit_has_diff_actions(overlay: &DiffOverlayState) -> bool {
    commit_has_text_diff(overlay) || diff_context_path(overlay).is_some()
}

pub(crate) fn commit_has_text_diff(overlay: &DiffOverlayState) -> bool {
    let DiffLoad::Ready(snapshot) = &overlay.load else {
        return false;
    };
    if let Some(path) = overlay.focus.as_deref() {
        if let Some(file) = snapshot.files.iter().find(|file| file.path == path) {
            return !file.binary && !file.hunks.is_empty();
        }
        return overlay.preview.as_ref().is_some_and(|(held, file)| {
            held == path
                && file
                    .as_ref()
                    .is_some_and(|file| !file.binary && !file.hunks.is_empty())
        });
    }
    snapshot
        .files
        .iter()
        .any(|file| !file.binary && !file.hunks.is_empty())
}

fn empty_snapshot(snap: &DiffSnapshot) -> bool {
    snap.files.is_empty() && snap.untracked.is_empty()
}

fn file_expanded(file: &FileDiff, expanded: &HashMap<String, bool>, collapse_all: bool) -> bool {
    if let Some(&want) = expanded.get(&file.path) {
        return want;
    }
    !collapse_all && file.added + file.removed <= AUTO_COLLAPSE_LINES
}

fn oversized_summary(snap: &DiffSnapshot, stats: &DiffStats) -> String {
    let mut parts = vec![t_plural(L10nKey::DiffChangedFiles, snap.files.len(), &[])];
    let (added, removed) = stats.totals;
    let total_lines = (added + removed) as usize;
    let loaded = stats.retained_lines;
    let budget = stats.budget_exhausted;
    let per_file = stats.per_file_truncated;
    parts.push(match (budget, per_file) {
        (false, false) => t_plural(L10nKey::DiffLines, total_lines, &[]),
        _ => {
            let cap_key = match (budget, per_file) {
                (true, true) => L10nKey::DiffBudgetAndCap,
                (true, false) => L10nKey::DiffBudget,
                _ => L10nKey::DiffPerFileCap,
            };
            t_fmt(
                L10nKey::DiffChangedLines,
                &[
                    ("total", &total_lines.to_string()),
                    ("loaded", &loaded.to_string()),
                    ("cap", t(cap_key)),
                ],
            )
        }
    });
    if stats.untracked_count > 0 {
        parts.push(t_plural(
            L10nKey::DiffUntrackedSummary,
            stats.untracked_count,
            &[],
        ));
    }
    parts.join(", ")
}

/// The de-duplication sets on `Tty7App` are keyed by `(HostId, PathBuf)`, so
/// the source rides along inside the path: two sources over one directory are
/// two independent probes and must not cancel one another.
///
/// `DiffSource::tag` rather than `Debug`, which is what this used to be built
/// from. `Debug` prints a commit's label too, so the same commit opened with a
/// subject in hand and without one would have been two keys and two probes for
/// one patch — the same split `DiffSource`'s own `PartialEq` is written to
/// avoid. The separator is a byte no path contains.
fn probe_key(
    host: crate::ui::host_ops::HostId,
    cwd: &Path,
    source: &DiffSource,
) -> (crate::ui::host_ops::HostId, PathBuf) {
    let mut tagged = std::ffi::OsString::from(format!("{}\u{1}", source.tag()));
    tagged.push(cwd.as_os_str());
    (host, PathBuf::from(tagged))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::git_diff::{DiffLine, LineKind};
    use crate::ui::i18n::set_locale;

    #[test]
    fn full_window_diff_background_is_opaque_with_or_without_a_preset() {
        let active = crate::ui::presets::ActiveBackground {
            fill: crate::ui::presets::Fill::Solid(0x12_34_56),
            opacity: Some(0.2),
            image: None,
        };
        let mut fallback: Hsla = gpui::rgb(0x65_43_21).into();
        fallback.a = 0.3;
        let mut opaque_fallback = fallback;
        opaque_fallback.a = 1.0;

        assert_eq!(
            diff_overlay_background(Some(&active), fallback),
            crate::ui::theme::window_background_opaque(&active),
            "the active preset must keep its fill while discarding workspace translucency"
        );
        assert_eq!(
            diff_overlay_background(None, fallback),
            opaque_fallback.into(),
            "the theme fallback must also block the window material"
        );
    }

    fn line(kind: LineKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
        DiffLine {
            kind,
            old_no: old,
            new_no: new,
            text: text.to_string(),
        }
    }

    #[test]
    fn the_probe_key_separates_the_sources_over_one_directory() {
        let host = crate::ui::host_ops::HostId::LOCAL;
        let cwd = Path::new("/repo");
        let worktree = probe_key(host, cwd, &DiffSource::Worktree);
        assert_ne!(worktree, probe_key(host, cwd, &DiffSource::Staged));
        assert_ne!(worktree, probe_key(host, cwd, &DiffSource::Head));
        assert_ne!(
            probe_key(host, cwd, &DiffSource::commit("a")),
            probe_key(host, cwd, &DiffSource::commit("b")),
            "two commits are two probes"
        );
        assert_eq!(worktree, probe_key(host, cwd, &DiffSource::Worktree));
        assert_ne!(
            worktree,
            probe_key(host, Path::new("/other"), &DiffSource::Worktree)
        );
        // …and one commit is one probe however much is known about it. Built
        // from `Debug`, as this key once was, the labelled one would have been
        // a second in-flight probe for a patch already being read.
        assert_eq!(
            probe_key(host, cwd, &DiffSource::commit("a")),
            probe_key(
                host,
                cwd,
                &DiffSource::Commit {
                    rev: "a".into(),
                    label: Some(CommitLabel {
                        subject: "s".into(),
                        author: "Ada".into(),
                        at: 1,
                    }),
                }
            )
        );
    }

    #[test]
    fn every_file_status_lands_on_a_shared_decoration() {
        use DecoStatus as D;
        for (status, want) in [
            (FileStatus::Added, D::Added),
            (FileStatus::Modified, D::Modified),
            (FileStatus::Deleted, D::Deleted),
            (FileStatus::Renamed, D::Renamed),
            // A copy is a rename that left the original behind: same letter.
            (FileStatus::Copied, D::Renamed),
            // A symlink that became a file is a modification, not a category
            // of its own — the overlay is the only place that ever saw a `T`.
            (FileStatus::TypeChanged, D::Modified),
            (FileStatus::Unmerged, D::Conflict),
        ] {
            assert_eq!(deco_status(status), want, "{status:?}");
        }
        assert_eq!(status_glyph(deco_status(FileStatus::Unmerged)), "U");
        assert_eq!(status_glyph(deco_status(FileStatus::Copied)), "R");
    }

    #[test]
    fn the_change_column_uses_the_typographic_minus() {
        assert_eq!(unified_marker(LineKind::Added), "+");
        assert_eq!(unified_marker(LineKind::Removed), "\u{2212}");
        assert_ne!(
            unified_marker(LineKind::Removed),
            "-",
            "the ASCII hyphen is narrower than `+`, and the column would wobble"
        );
        assert_eq!(
            unified_marker(LineKind::Context),
            "",
            "a context line is neither, and a placeholder glyph would be noise"
        );
    }

    #[test]
    fn the_header_shortens_an_object_id_and_nothing_else() {
        let oid = "3f2a1b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a";
        assert_eq!(short_rev(oid), "3f2a1b9c");
        assert_eq!(short_rev("v26.7.5"), "v26.7.5");
        assert_eq!(
            short_rev("origin/main"),
            "origin/main",
            "half a ref name says less than the whole of it"
        );
        assert_eq!(short_rev("3f2a1b9"), "3f2a1b9", "already short");
    }

    #[test]
    fn each_source_names_itself_in_the_header() {
        let branch = || "main".to_string();
        let plain = source_subject(&DiffSource::Worktree, branch());
        assert_eq!((plain.icon, plain.text.as_str()), (BRANCH_ICON, "main"));
        assert_eq!(plain.chip, None);
        assert!(!plain.is_rev);

        assert_eq!(source_subject(&DiffSource::Head, branch()).chip, None);

        let staged = source_subject(&DiffSource::Staged, branch());
        assert_eq!(staged.icon, BRANCH_ICON, "still a branch, still its name");
        assert_eq!(
            staged.chip,
            Some("STAGED"),
            "without it the staged patch is indistinguishable from the unstaged one"
        );

        let commit = source_subject(
            &DiffSource::commit("3f2a1b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a"),
            branch(),
        );
        assert_eq!(commit.icon, COMMIT_ICON);
        assert_eq!(commit.text, "3f2a1b9c", "the branch is not what is shown");
        assert!(commit.is_rev);

        let range = source_subject(
            &DiffSource::Range {
                base: "main".into(),
                head: "feature".into(),
            },
            branch(),
        );
        assert_eq!(range.icon, COMMIT_ICON);
        assert_eq!(range.text, "main…feature");
    }

    #[test]
    fn a_labelled_commit_says_what_it_was_about() {
        let label = CommitLabel {
            subject: "fix(scm): stop the panel asking twice".into(),
            author: "Ada".into(),
            at: 1_786_255_391,
        };
        let with = source_subject(
            &DiffSource::Commit {
                rev: "3f2a1b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a".into(),
                label: Some(label.clone()),
            },
            "main".to_string(),
        );
        assert_eq!(with.text, "3f2a1b9c", "the sha is still the identifier");
        assert_eq!(
            with.label.as_ref().map(|l| l.subject.as_str()),
            Some(label.subject.as_str())
        );

        // Nothing else grows a subject line, least of all a working-tree
        // patch, whose "subject" would be a branch name repeated.
        assert!(
            source_subject(&DiffSource::Worktree, "main".into())
                .label
                .is_none()
        );
        assert!(
            source_subject(&DiffSource::Head, "main".into())
                .label
                .is_none()
        );
        assert!(
            source_subject(&DiffSource::commit("deadbeef"), "main".into())
                .label
                .is_none(),
            "a commit nobody has read yet has nothing to say"
        );
        // A default-constructed label is indistinguishable from none, and must
        // not paint an empty row where the subject would go.
        let empty = source_subject(
            &DiffSource::Commit {
                rev: "deadbeef".into(),
                label: Some(CommitLabel::default()),
            },
            "main".into(),
        );
        assert!(empty.label.is_none());
    }

    #[test]
    fn the_byline_drops_the_separator_along_with_the_half_it_joined() {
        let now = 1_786_255_391 + 7200;
        let full = CommitLabel {
            subject: "s".into(),
            author: "Ada".into(),
            at: 1_786_255_391,
        };
        assert_eq!(label_byline(&full, now), "Ada · 2h");
        assert_eq!(
            label_byline(
                &CommitLabel {
                    author: String::new(),
                    ..full.clone()
                },
                now
            ),
            "2h",
            "a commit with no author is not `· 2h`"
        );
        assert_eq!(
            label_byline(&CommitLabel { at: 0, ..full }, now),
            "Ada",
            "and a timestamp that would not parse is not `Ada · 56y`"
        );
    }

    const BRANCH_ICON: &str = "icons/git-branch.svg";
    const COMMIT_ICON: &str = "icons/git-commit.svg";

    fn small_file(path: &str, added: u32) -> FileDiff {
        FileDiff {
            path: path.to_string(),
            old_path: None,
            status: FileStatus::Modified,
            added,
            removed: 0,
            binary: false,
            truncated: None,
            hunks: vec![git_diff::Hunk {
                header: "@@ -1,1 +1,1 @@".to_string(),
                lines: (0..added)
                    .map(|i| line(LineKind::Added, None, Some(i + 1), "x"))
                    .collect(),
            }],
        }
    }

    fn context_heavy_file(path: &str) -> FileDiff {
        FileDiff {
            path: path.to_string(),
            old_path: None,
            status: FileStatus::Modified,
            added: 1,
            removed: 0,
            binary: false,
            truncated: None,
            hunks: vec![git_diff::Hunk {
                header: "@@ -1,7 +1,7 @@".to_string(),
                lines: (0..6)
                    .map(|i| line(LineKind::Context, Some(i + 1), Some(i + 1), "ctx"))
                    .chain(std::iter::once(line(LineKind::Added, None, Some(7), "x")))
                    .collect(),
            }],
        }
    }

    fn choices<const N: usize>(pairs: [(&str, bool); N]) -> HashMap<String, bool> {
        pairs.into_iter().map(|(p, v)| (p.to_string(), v)).collect()
    }

    fn banner(snap: &DiffSnapshot) -> String {
        set_locale("en");
        oversized_summary(snap, &snap.stats())
    }

    #[test]
    fn per_file_collapse_is_unchanged_below_the_repo_threshold() {
        let small = small_file("small.rs", 10);
        let big = small_file("big.rs", AUTO_COLLAPSE_LINES + 1);
        let none = HashMap::new();
        assert!(file_expanded(&small, &none, false));
        assert!(!file_expanded(&big, &none, false));

        let picked = choices([("small.rs", false), ("big.rs", true)]);
        assert!(!file_expanded(&small, &picked, false));
        assert!(file_expanded(&big, &picked, false));
    }

    #[test]
    fn repo_wide_collapse_overrides_the_per_file_default() {
        let small = small_file("small.rs", 10);
        let none = HashMap::new();
        assert!(file_expanded(&small, &none, false));
        assert!(!file_expanded(&small, &none, true), "collapsed en masse");

        assert!(
            file_expanded(&small, &choices([("small.rs", true)]), true),
            "the user's own click still opens it"
        );
    }

    #[test]
    fn explicit_choices_survive_an_oversized_transition() {
        let opened = small_file("opened.rs", 10);
        let closed = small_file("closed.rs", 10);
        let untouched = small_file("untouched.rs", 10);
        let picked = choices([("opened.rs", true), ("closed.rs", false)]);

        for collapse_all in [true, false] {
            assert!(
                file_expanded(&opened, &picked, collapse_all),
                "an explicitly opened file stays open (collapse_all={collapse_all})"
            );
            assert!(
                !file_expanded(&closed, &picked, collapse_all),
                "an explicitly closed file stays closed (collapse_all={collapse_all})"
            );
        }
        assert!(!file_expanded(&untouched, &picked, true));
        assert!(file_expanded(&untouched, &picked, false));
    }

    #[test]
    fn many_medium_files_are_oversized_and_build_no_rows() {
        let snap = DiffSnapshot {
            files: (0..60)
                .map(|i| small_file(&format!("f{i}.rs"), 150))
                .collect(),
            ..Default::default()
        };
        assert!(
            snap.files.iter().all(|f| f.added <= AUTO_COLLAPSE_LINES),
            "no single file is over the per-file threshold"
        );
        assert!(snap.stats().oversized);

        let none = HashMap::new();
        let rows_expanded: usize = snap
            .files
            .iter()
            .filter(|f| file_expanded(f, &none, false))
            .flat_map(|f| &f.hunks)
            .map(|h| split_hunk(&h.lines).len())
            .sum();
        let rows_collapsed: usize = snap
            .files
            .iter()
            .filter(|f| file_expanded(f, &none, true))
            .flat_map(|f| &f.hunks)
            .map(|h| split_hunk(&h.lines).len())
            .sum();
        assert_eq!(rows_expanded, 9000, "what the old rule would have built");
        assert_eq!(rows_collapsed, 0);
    }

    #[test]
    fn an_ordinary_busy_tree_is_not_oversized() {
        let snap = DiffSnapshot {
            files: (0..40)
                .map(|i| {
                    let mut f = context_heavy_file(&format!("f{i}.rs"));
                    f.hunks = std::iter::repeat_n(f.hunks[0].clone(), 10).collect();
                    f.added = 10;
                    f
                })
                .collect(),
            ..Default::default()
        };
        let (added, removed) = snap.totals();
        assert!(
            snap.stats().retained_lines > (added + removed) as usize * 4,
            "the context lines dominate, as they do in a real diff"
        );
        assert!(
            !snap.stats().oversized,
            "an ordinary afternoon must not read as a tree too large to render \
             ({} retained lines)",
            snap.stats().retained_lines
        );
        let none = HashMap::new();
        assert!(snap.files.iter().all(|f| file_expanded(f, &none, false)));
    }

    #[test]
    fn an_empty_snapshot_reads_as_clean_only_when_the_probe_worked() {
        let clean = DiffSnapshot {
            branch: "main".into(),
            ..Default::default()
        };
        assert!(empty_snapshot(&clean));
        assert!(!clean.read_failed, "nothing went wrong: this tree is clean");

        let broken = DiffSnapshot {
            branch: "main".into(),
            read_failed: true,
            ..Default::default()
        };
        assert!(
            empty_snapshot(&broken),
            "indistinguishable by shape — which is the point"
        );
        assert!(broken.read_failed, "and distinguishable by this");

        let partial = DiffSnapshot {
            files: vec![small_file("one.rs", 3)],
            read_failed: true,
            ..Default::default()
        };
        assert!(!empty_snapshot(&partial));
    }

    #[test]
    fn a_huge_untracked_list_does_not_collapse_the_diff() {
        let snap = DiffSnapshot {
            files: vec![small_file("one.rs", 3)],
            untracked: (0..git_diff::MAX_UNTRACKED)
                .map(|i| format!("node_modules/p{i}/index.js"))
                .collect(),
            untracked_total: 40_000,
            ..Default::default()
        };
        assert!(snap.stats().retained_lines < git_diff::AUTO_COLLAPSE_TOTAL_LINES);
        assert!(snap.files.len() < git_diff::AUTO_COLLAPSE_TOTAL_FILES);
        assert!(
            !snap.stats().oversized,
            "collapsing the diff would not have removed a single untracked row"
        );

        assert_eq!(
            snap.untracked.len(),
            git_diff::MAX_UNTRACKED,
            "retention is capped at the parser"
        );
        assert_eq!(
            snap.untracked.len().min(MAX_RENDERED_FILES),
            MAX_RENDERED_FILES,
            "and rows at the renderer"
        );
        assert_eq!(
            snap.untracked_count(),
            40_000,
            "while the reported count stays the true total"
        );
    }

    #[test]
    fn a_focused_untracked_file_asks_for_a_preview_not_the_list() {
        let snap = DiffSnapshot {
            files: vec![small_file("tracked.rs", 3)],
            untracked: vec!["new.md".to_string()],
            ..Default::default()
        };
        assert_eq!(untracked_focus(&snap, Some("new.md")), Some("new.md"));
        assert_eq!(
            untracked_focus(&snap, Some("tracked.rs")),
            None,
            "a real patch wins over the name list"
        );
        assert_eq!(untracked_focus(&snap, Some("absent.rs")), None);
        assert_eq!(untracked_focus(&snap, None), None);
    }

    #[test]
    fn untracked_rows_are_capped_but_the_count_stays_true() {
        let snap = DiffSnapshot {
            untracked: (0..git_diff::MAX_UNTRACKED)
                .map(|i| format!("p{i}"))
                .collect(),
            untracked_total: 12_345,
            ..Default::default()
        };
        let rendered = snap.untracked.len().min(MAX_RENDERED_FILES);
        assert_eq!(rendered, MAX_RENDERED_FILES);
        assert_eq!(snap.untracked_count(), 12_345, "header count is honest");
        assert_eq!(snap.untracked_count() - rendered, 12_045);
    }

    #[test]
    fn untracked_count_falls_back_to_the_retained_length() {
        let snap = DiffSnapshot {
            untracked: vec!["a".into(), "b".into(), "c".into()],
            ..Default::default()
        };
        assert_eq!(snap.untracked_total, 0);
        assert_eq!(snap.untracked_count(), 3);
    }

    #[test]
    fn file_cards_are_capped() {
        let snap = DiffSnapshot {
            files: (0..MAX_RENDERED_FILES + 25)
                .map(|i| small_file(&format!("f{i}.rs"), 1))
                .collect(),
            ..Default::default()
        };
        let shown = snap.files.len().min(MAX_RENDERED_FILES);
        assert_eq!(shown, MAX_RENDERED_FILES);
        assert_eq!(
            snap.files.len() - shown,
            25,
            "the tail gets one summary line"
        );
    }

    #[test]
    fn the_banner_names_the_budget_when_context_outweighs_the_changes() {
        let files: Vec<FileDiff> = (0..200)
            .map(|i| context_heavy_file(&format!("f{i}.rs")))
            .collect();

        let mut truncated = files.clone();
        truncated.push(FileDiff {
            hunks: vec![],
            truncated: Some(Truncation::Budget),
            ..context_heavy_file("dropped.rs")
        });
        let snap = DiffSnapshot {
            files: truncated,
            ..Default::default()
        };
        let (added, removed) = snap.totals();
        let total = (added + removed) as usize;
        assert!(
            snap.stats().retained_lines > total,
            "the context lines outweigh the changed ones — the shape that slipped through"
        );
        assert!(snap.stats().budget_exhausted);

        let summary = banner(&snap);
        assert!(summary.contains("budget"), "{summary}");
        assert!(
            summary.contains(&format!("{total} changed lines")),
            "the exact `+N −N` from the header, not the retained count: {summary}"
        );

        let whole = DiffSnapshot {
            files,
            ..Default::default()
        };
        assert!(!whole.stats().budget_exhausted);
        assert!(!banner(&whole).contains("budget"));
    }

    #[test]
    fn the_banner_names_the_per_file_cap_when_context_outweighs_the_changes() {
        let files: Vec<FileDiff> = (0..200)
            .map(|i| context_heavy_file(&format!("f{i}.rs")))
            .collect();

        let mut cut = files.clone();
        cut.push(FileDiff {
            truncated: Some(Truncation::PerFile),
            ..context_heavy_file("huge.rs")
        });
        let snap = DiffSnapshot {
            files: cut,
            ..Default::default()
        };
        let (added, removed) = snap.totals();
        let total = (added + removed) as usize;
        assert!(
            snap.stats().retained_lines > total,
            "the shape the comparison reads backwards"
        );
        assert!(
            !snap.stats().budget_exhausted,
            "the budget axis is not what fired"
        );

        let summary = banner(&snap);
        assert!(summary.contains("per-file cap"), "{summary}");
        assert!(
            summary.contains(&format!("{total} changed lines")),
            "the exact `+N −N` from the header, not the retained count: {summary}"
        );

        let whole = DiffSnapshot {
            files: files.clone(),
            ..Default::default()
        };
        assert!(!banner(&whole).contains("per-file"));

        let mut both = files;
        both.push(FileDiff {
            truncated: Some(Truncation::PerFile),
            ..context_heavy_file("huge.rs")
        });
        both.push(FileDiff {
            hunks: vec![],
            truncated: Some(Truncation::Budget),
            ..context_heavy_file("dropped.rs")
        });
        let summary = banner(&DiffSnapshot {
            files: both,
            ..Default::default()
        });
        assert!(summary.contains("budget"), "{summary}");
        assert!(summary.contains("per-file cap"), "{summary}");
    }

    #[test]
    #[ignore = "measurement, not an assertion"]
    fn bench_snapshot_share() {
        use std::time::Instant;

        for (label, files, per_file) in [
            ("unbudgeted (v26.7.5 shape)", 300, 300),
            ("budgeted (this build retains)", 300, 67),
        ] {
            let snap = Arc::new(DiffSnapshot {
                files: (0..files)
                    .map(|i| small_file(&format!("f{i}.rs"), per_file))
                    .collect(),
                ..Default::default()
            });
            let lines: usize = snap.stats().retained_lines;

            let t = Instant::now();
            for _ in 0..10 {
                let _deep = (*snap).clone();
            }
            let deep = t.elapsed() / 10;

            let t = Instant::now();
            for _ in 0..100 {
                let _shared = Arc::clone(&snap);
            }
            let shared = t.elapsed() / 100;
            println!(
                "{label}: {files} files / {lines} lines — deep clone {deep:?} vs \
                 Arc::clone {shared:?}, per holder on the UI thread"
            );
        }
    }
}

#[cfg(all(test, unix))]
mod overlay_gpui_tests {
    use super::*;
    use crate::ui::app::test_window;
    use crate::ui::host_ops::HostId;
    use gpui::{Entity, TestAppContext, VisualTestContext};

    fn overlay_source_and_load(
        app: &Entity<Tty7App>,
        vcx: &mut VisualTestContext,
    ) -> (DiffSource, bool) {
        app.update_in(vcx, |app, _, _| {
            let overlay = app.tabs[app.active]
                .diff_overlay
                .as_ref()
                .expect("an overlay is open");
            (
                overlay.source.clone(),
                matches!(overlay.load, DiffLoad::Loading),
            )
        })
    }

    /// Opening a staged file while a worktree overlay is up must re-probe. The
    /// filter used to match on `(cwd, host)` alone, so it took the "just move
    /// the focus" branch and left the unstaged patch on screen under the
    /// staged file's name.
    #[gpui::test]
    fn a_second_source_over_one_directory_is_a_second_overlay(cx: &mut TestAppContext) {
        let (app, mut vcx, _pane) = test_window::harness_with_tabs(cx, 1);
        let cwd = std::path::PathBuf::from("/no/such/tty7/repo");

        app.update_in(&mut vcx, |app, window, cx| {
            app.open_diff_overlay(
                HostId::LOCAL,
                cwd.clone(),
                DiffSource::Worktree,
                Some("a.rs".to_string()),
                window,
                cx,
            );
        });
        assert_eq!(
            overlay_source_and_load(&app, &mut vcx),
            (DiffSource::Worktree, true)
        );

        // Pretend the worktree probe landed, so a reused overlay would show it.
        app.update_in(&mut vcx, |app, _, _| {
            let active = app.active;
            let overlay = app.tabs[active].diff_overlay.as_mut().unwrap();
            overlay.loading = false;
            overlay.load = DiffLoad::Ready(Arc::new(DiffSnapshot {
                source: DiffSource::Worktree,
                branch: "main".into(),
                ..Default::default()
            }));
        });

        app.update_in(&mut vcx, |app, window, cx| {
            app.open_diff_overlay(
                HostId::LOCAL,
                cwd.clone(),
                DiffSource::Staged,
                Some("a.rs".to_string()),
                window,
                cx,
            );
        });
        assert_eq!(
            overlay_source_and_load(&app, &mut vcx),
            (DiffSource::Staged, true),
            "the same file from a different source is a different question"
        );
    }

    fn one_file_snapshot(source: DiffSource) -> DiffSnapshot {
        use crate::terminal::git_diff::{DiffLine, Hunk, LineKind};
        DiffSnapshot {
            root: std::path::PathBuf::from("/no/such/tty7/repo"),
            source,
            branch: "main".into(),
            files: vec![FileDiff {
                path: "a.rs".into(),
                old_path: None,
                status: FileStatus::Modified,
                added: 1,
                removed: 1,
                binary: false,
                truncated: None,
                hunks: vec![Hunk {
                    header: "@@ -1,2 +1,2 @@".into(),
                    lines: vec![
                        DiffLine {
                            kind: LineKind::Context,
                            old_no: Some(1),
                            new_no: Some(1),
                            text: "keep".into(),
                        },
                        DiffLine {
                            kind: LineKind::Removed,
                            old_no: Some(2),
                            new_no: None,
                            text: "old".into(),
                        },
                        DiffLine {
                            kind: LineKind::Added,
                            old_no: None,
                            new_no: Some(2),
                            text: "new".into(),
                        },
                    ],
                }],
            }],
            untracked: vec!["scratch.txt".into()],
            untracked_total: 1,
            read_failed: false,
        }
    }

    fn show(app: &Entity<Tty7App>, vcx: &mut VisualTestContext, source: DiffSource) {
        let cwd = std::path::PathBuf::from("/no/such/tty7/repo");
        app.update_in(vcx, |app, window, cx| {
            app.open_diff_overlay(HostId::LOCAL, cwd.clone(), source.clone(), None, window, cx);
            let active = app.active;
            let overlay = app.tabs[active].diff_overlay.as_mut().unwrap();
            overlay.loading = false;
            overlay.load = DiffLoad::Ready(Arc::new(one_file_snapshot(source)));
            // The card is what carries the rows, so open it.
            overlay.expanded.insert("a.rs".to_string(), true);
        });
    }

    /// Every header branch, every row renderer, once each. A missing icon, an
    /// unset global or a panicking helper shows up here rather than the first
    /// time somebody opens a commit.
    #[gpui::test]
    fn every_source_renders_in_both_views(cx: &mut TestAppContext) {
        let (app, mut vcx, _pane) = test_window::harness_with_tabs(cx, 1);

        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            app.update_in(&mut vcx, |app, _, cx| {
                app.update_config(cx, |cfg| cfg.diff_view = mode);
            });
            for source in [
                DiffSource::Worktree,
                DiffSource::Staged,
                DiffSource::Head,
                DiffSource::commit("3f2a1b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a"),
                DiffSource::Commit {
                    rev: "3f2a1b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a".into(),
                    label: Some(CommitLabel {
                        subject: "fix(scm): read a commit's own header".into(),
                        author: "Ada".into(),
                        at: 1_786_255_391,
                    }),
                },
                DiffSource::Range {
                    base: "main".into(),
                    head: "feature".into(),
                },
            ] {
                show(&app, &mut vcx, source.clone());
                // A real frame, so layout and paint run too: `title_bar_drag`
                // and the segmented track both want a window that is drawing.
                crate::ui::app::render_probe::arm(10_000);
                app.update_in(&mut vcx, |_, _, cx| cx.notify());
                vcx.background_executor.run_until_parked();
                assert!(
                    crate::ui::app::render_probe::draws() > 0,
                    "nothing was drawn, so nothing was proved: {source:?} in {mode:?}"
                );
                app.update_in(&mut vcx, |app, window, cx| {
                    app.close_diff_overlay(window, cx)
                });
            }
        }
    }

    #[gpui::test]
    fn toggling_the_diff_view_mode_writes_config(cx: &mut TestAppContext) {
        let (app, mut vcx, _pane) = test_window::harness_with_tabs(cx, 1);
        let mode =
            |vcx: &mut VisualTestContext| vcx.update(|_, cx| cx.global::<Config>().diff_view);

        assert_eq!(
            mode(&mut vcx),
            DiffViewMode::Split,
            "side by side is what everyone already sees"
        );

        app.update_in(&mut vcx, |app, _, cx| app.toggle_diff_view_mode(cx));
        assert_eq!(mode(&mut vcx), DiffViewMode::Unified);

        app.update_in(&mut vcx, |app, _, cx| app.toggle_diff_view_mode(cx));
        assert_eq!(mode(&mut vcx), DiffViewMode::Split, "and back again");
    }

    /// Focusing an untracked file has to leave a way back to the list.
    ///
    /// Its card is synthesized from the file's own bytes, so it has no entry in
    /// `files` — and the header's breadcrumb read only `files`. The one view
    /// that could be entered was the one view that could not be left except by
    /// closing the overlay and opening it again.
    #[gpui::test]
    fn a_focused_untracked_file_still_has_a_way_back(cx: &mut TestAppContext) {
        let (app, mut vcx, _pane) = test_window::harness_with_tabs(cx, 1);
        let cwd = std::path::PathBuf::from("/no/such/tty7/repo");

        app.update_in(&mut vcx, |app, window, cx| {
            app.open_diff_overlay(
                HostId::LOCAL,
                cwd.clone(),
                DiffSource::Worktree,
                Some("scratch.txt".to_string()),
                window,
                cx,
            );
            let active = app.active;
            let overlay = app.tabs[active].diff_overlay.as_mut().unwrap();
            overlay.loading = false;
            overlay.load = DiffLoad::Ready(Arc::new(DiffSnapshot {
                source: DiffSource::Worktree,
                branch: "main".into(),
                untracked: vec!["scratch.txt".to_string()],
                untracked_total: 1,
                ..Default::default()
            }));
        });

        app.update_in(&mut vcx, |app, _, _| {
            let overlay = app.tabs[app.active].diff_overlay.as_ref().unwrap();
            assert_eq!(
                focused_name(overlay).as_deref(),
                Some("scratch.txt"),
                "the breadcrumb is the way back"
            );
        });
    }

    /// The same source and the same focus still toggles the overlay shut.
    #[gpui::test]
    fn the_same_source_twice_still_closes(cx: &mut TestAppContext) {
        let (app, mut vcx, _pane) = test_window::harness_with_tabs(cx, 1);
        let cwd = std::path::PathBuf::from("/no/such/tty7/repo");

        for _ in 0..2 {
            app.update_in(&mut vcx, |app, window, cx| {
                app.open_diff_overlay(
                    HostId::LOCAL,
                    cwd.clone(),
                    DiffSource::Worktree,
                    None,
                    window,
                    cx,
                );
            });
        }
        app.update_in(&mut vcx, |app, _, _| {
            assert!(app.tabs[app.active].diff_overlay.is_none());
        });
    }
}

/// An overlay that has read its patch has to stop reading it.
///
/// `maybe_refresh_diff_overlay` calls a `Head` overlay stale when the cached
/// git status disagrees with the snapshot on screen, and every landed probe
/// wakes that check by touching the status cache. So a disagreement the probe
/// cannot settle is not a stale badge — it is a loop: read the diff, publish
/// it, wake the watchers, find the same disagreement, read it again. Two `git`
/// processes a lap, `refreshing…` pinned to the header, and a window that
/// costs 7% of a core sitting still.
///
/// The way in is ordinary: switch branches anywhere outside tty7 — another
/// terminal, an editor, a worktree command — and the cached branch is a branch
/// the repository has left. That is what the stale entry below stands for.
///
/// Unix-gated like every other window harness in this tree: `harness_with_tabs`
/// hands back a `std::os::unix::net::UnixStream` for the pane.
#[cfg(all(test, unix))]
mod render_idle_gpui_tests {
    use super::*;
    use crate::terminal::git_status::{GitStatusCache, RepoSnapshot};
    use crate::ui::app::{render_probe, test_window};
    use crate::ui::host_ops::HostId;
    use gpui::TestAppContext;

    const BUDGET: u64 = 200;

    fn git(root: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?} failed");
    }

    #[gpui::test]
    fn an_overlay_over_a_stale_branch_reaches_render_idle(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("tty7-diff-idle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(&root).unwrap();
        git(&root, &["init", "--quiet"]);
        std::fs::write(root.join("a.rs"), "fn main() {}\n").unwrap();
        git(&root, &["add", "a.rs"]);
        git(
            &root,
            &[
                "-c",
                "user.email=t@x",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "one",
            ],
        );
        // Something for the overlay to actually show, so it settles on a
        // snapshot rather than on the empty state.
        std::fs::write(root.join("a.rs"), "fn main() { /* edited */ }\n").unwrap();

        let (app, mut vcx, _pane) = test_window::harness_with_tabs(cx, 1);

        // The cache as a branch switch outside tty7 leaves it: a branch this
        // repository is no longer on, and counts from before the switch.
        let stale = root.clone();
        app.update_in(&mut vcx, |_, _, cx| {
            cx.default_global::<GitStatusCache>();
            cx.update_global::<GitStatusCache, _>(|cache, _| {
                cache.finish_probe(
                    HostId::LOCAL,
                    &stale,
                    Some(RepoSnapshot {
                        root: stale.clone(),
                        home: stale.clone(),
                        branch: "a-branch-this-repo-has-left".into(),
                        counts: Some((99, 99)),
                    }),
                );
            });
        });

        let open = root.clone();
        app.update_in(&mut vcx, |app, window, cx| {
            app.open_diff_overlay(HostId::LOCAL, open, DiffSource::Head, None, window, cx);
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            vcx.background_executor.run_until_parked();
            let ready = app.update_in(&mut vcx, |app, _, _| {
                app.tabs[app.active]
                    .diff_overlay
                    .as_ref()
                    .is_some_and(|o| matches!(o.load, DiffLoad::Ready(_)) && !o.loading)
            });
            if ready {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the overlay never landed a snapshot"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        test_window::quiesce(&mut vcx, Some(&root));
        render_probe::arm(BUDGET);
        vcx.background_executor.run_until_parked();
        vcx.executor()
            .advance_clock(std::time::Duration::from_secs(3));
        vcx.background_executor.run_until_parked();
        render_probe::arm(BUDGET);
        vcx.executor()
            .advance_clock(std::time::Duration::from_secs(9));
        vcx.background_executor.run_until_parked();

        assert_eq!(
            render_probe::draws(),
            0,
            "a settled overlay must stop re-reading its own diff"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod github_theme_gpui_tests {
    use super::*;
    use crate::ui::app::test_window;
    use gpui::{TestAppContext, point, size};
    use gpui_component::{Theme, ThemeMode};

    struct ContextDiff(gpui::Entity<Tty7App>);

    impl gpui::Render for ContextDiff {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let overlay = self.0.update(cx, |app, cx| {
                app.render_diff_overlay(DocumentChrome::Fill, window, cx)
                    .unwrap_or_else(|| div().into_any_element())
            });
            v_flex().size_full().child(overlay)
        }
    }

    #[gpui::test]
    fn expanding_file_context_keeps_the_full_diff_list(cx: &mut TestAppContext) {
        let (app, mut initial_window) = test_window::harness(cx);
        initial_window.update(|window, _| window.remove_window());

        let first = git_diff::synthesize_added(
            "first.rs",
            b"fn first() {}\n",
            &git_diff::DiffBudget::SINGLE_FILE,
        );
        let second = git_diff::synthesize_added(
            "second.rs",
            b"fn second() {}\n",
            &git_diff::DiffBudget::SINGLE_FILE,
        );
        let mut full_second = git_diff::synthesize_added(
            "second.rs",
            (1..=40)
                .map(|n| format!("fn second_{n}() {{}}\n"))
                .collect::<String>()
                .as_bytes(),
            &git_diff::DiffBudget::SINGLE_FILE,
        );
        full_second.status = FileStatus::Modified;
        let full_second = Arc::new(full_second);
        let snapshot = Arc::new(DiffSnapshot {
            files: vec![first.clone(), second.clone()],
            source: DiffSource::Staged,
            ..Default::default()
        });

        let (_, vcx) = cx.add_window_view(|_, _| ContextDiff(app.clone()));
        vcx.simulate_resize(size(px(1000.), px(600.)));
        app.update_in(vcx, |app, _window, cx| {
            cx.global_mut::<Config>().diff_view = DiffViewMode::Split;
            app.tabs = vec![crate::ui::app::Tab::new(crate::ui::pane::Pane::Empty)];
            app.active = 0;
            app.tabs[0].diff_overlay = Some(DiffOverlayState {
                pane_host: None,
                host_id: crate::ui::host_ops::HostId::LOCAL,
                cwd: PathBuf::from("/fixture"),
                source: DiffSource::Staged,
                commit: None,
                commit_message_expanded: false,
                commit_split: crate::ui::diff_overlay::CommitSplit::new(cx),
                document_header_focus: cx.focus_handle(),
                focus_handle: cx.focus_handle(),
                load: DiffLoad::Ready(snapshot),
                loading: false,
                expanded: HashMap::from([("first.rs".into(), true), ("second.rs".into(), true)]),
                focus: None,
                preview: None,
                preview_loading: None,
                scroll: gpui::ScrollHandle::new(),
                epoch: None,
                file_views: HashMap::from([
                    (
                        "first.rs".into(),
                        DiffFileView {
                            whole: false,
                            revealed: vec![4..8],
                            full: Some(Arc::new(first.clone())),
                            shown: Some(Arc::new(first)),
                            loading: false,
                            failed: false,
                            token: Rc::new(()),
                        },
                    ),
                    (
                        "second.rs".into(),
                        DiffFileView {
                            whole: false,
                            revealed: vec![],
                            full: Some(full_second.clone()),
                            shown: Some(Arc::new(second)),
                            loading: false,
                            failed: false,
                            token: Rc::new(()),
                        },
                    ),
                ]),
                geometry: Rc::default(),
                anchor: Rc::default(),
            });
            cx.notify();
        });
        vcx.run_until_parked();

        assert!(vcx.debug_bounds("diff-card-0").is_some());
        assert!(vcx.debug_bounds("diff-card-1").is_some());
        app.update(vcx, |app, cx| {
            app.change_diff_context("second.rs".into(), None, cx);
        });
        vcx.run_until_parked();

        app.update(vcx, |app, _| {
            let overlay = app.tabs[0].diff_overlay.as_ref().unwrap();
            assert_eq!(
                overlay.focus, None,
                "expanding a card is not file navigation"
            );
            let first = overlay.file_views.get("first.rs").unwrap();
            assert!(!first.whole, "other file context stays partially expanded");
            assert_eq!(first.revealed, vec![4..8]);
            let view = overlay.file_views.get("second.rs").unwrap();
            assert!(view.whole, "the file-level control expands this file");
        });
        assert!(vcx.debug_bounds("diff-card-0").is_some());
        assert!(vcx.debug_bounds("diff-card-1").is_some());
    }

    #[gpui::test]
    fn full_file_controls_preserve_change_position_and_layout(cx: &mut TestAppContext) {
        let (app, mut initial_window) = test_window::harness(cx);
        // One overlay scroll handle belongs to one window. The generic harness
        // window must not also lay out this fixture's shared application entity.
        initial_window.update(|window, _| window.remove_window());
        let mut full = git_diff::synthesize_added(
            "context.rs",
            (1..=100)
                .map(|n| format!("line {n}\n"))
                .collect::<String>()
                .as_bytes(),
            &git_diff::DiffBudget::SINGLE_FILE,
        );
        full.status = FileStatus::Modified;
        for line in &mut full.hunks[0].lines {
            line.kind = LineKind::Context;
            line.old_no = line.new_no;
        }
        for n in [25, 65].into_iter().rev() {
            let mut removed = full.hunks[0].lines[n - 1].clone();
            removed.kind = LineKind::Removed;
            removed.new_no = None;
            full.hunks[0].lines[n - 1].kind = LineKind::Added;
            full.hunks[0].lines[n - 1].old_no = None;
            full.hunks[0].lines.insert(n - 1, removed);
        }
        let full = Arc::new(full);
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            let (_, vcx) = cx.add_window_view(|_, _| ContextDiff(app.clone()));
            vcx.simulate_resize(size(px(1000.), px(600.)));
            app.update_in(vcx, |app, _window, cx| {
                cx.global_mut::<Config>().diff_view = mode;
                app.tabs = vec![crate::ui::app::Tab::new(crate::ui::pane::Pane::Empty)];
                app.active = 0;
                app.tabs[0].diff_overlay = Some(DiffOverlayState {
                    pane_host: None,
                    host_id: crate::ui::host_ops::HostId::LOCAL,
                    cwd: PathBuf::from("/fixture"),
                    source: DiffSource::Staged,
                    commit: None,
                    commit_message_expanded: false,
                    commit_split: crate::ui::diff_overlay::CommitSplit::new(cx),
                    document_header_focus: cx.focus_handle(),
                    focus: Some("context.rs".into()),
                    focus_handle: cx.focus_handle(),
                    load: DiffLoad::Loading,
                    loading: false,
                    expanded: HashMap::new(),
                    preview: None,
                    preview_loading: None,
                    scroll: gpui::ScrollHandle::new(),
                    epoch: None,
                    file_views: HashMap::new(),
                    geometry: Rc::default(),
                    anchor: Rc::default(),
                });
                let o = app.tabs[0].diff_overlay.as_mut().unwrap();
                o.loading = false;
                o.load = DiffLoad::Ready(Arc::new(DiffSnapshot {
                    files: vec![crate::ui::diff_context::with_context(&full, &[])],
                    source: DiffSource::Staged,
                    ..Default::default()
                }));
                o.file_views.insert(
                    "context.rs".into(),
                    DiffFileView {
                        whole: false,
                        revealed: vec![],
                        full: Some(full.clone()),
                        shown: None,
                        loading: false,
                        failed: false,
                        token: Rc::new(()),
                    },
                );
                cx.notify();
            });
            vcx.run_until_parked();
            let position = |vcx: &mut gpui::VisualTestContext| {
                app.update(vcx, |app, _| {
                    let o = app.tabs[0].diff_overlay.as_ref().unwrap();
                    let g = o.geometry.borrow();
                    let g = g.get("context.rs").unwrap();
                    g.content_top
                        + o.scroll.offset().y.as_f32()
                        + anchor_row(&g.rows, (None, Some(25))).unwrap().1
                })
            };
            let before = position(vcx);
            app.update(vcx, |app, cx| {
                app.change_diff_context("context.rs".into(), None, cx);
            });
            vcx.run_until_parked();
            for _ in 0..2 {
                vcx.update(|window, cx| {
                    window.refresh();
                    let _ = window.draw(cx);
                });
            }
            assert!(
                (position(vcx) - before).abs() < 1.,
                "expand moved the change in {mode:?}: {before} -> {}",
                position(vcx)
            );
            app.update(vcx, |app, cx| {
                assert_eq!(cx.global::<Config>().diff_view, mode);
                assert!(
                    app.tabs[0]
                        .diff_overlay
                        .as_ref()
                        .unwrap()
                        .file_views
                        .get("context.rs")
                        .unwrap()
                        .whole
                );
                app.change_diff_context("context.rs".into(), None, cx);
            });
            vcx.run_until_parked();
            for _ in 0..2 {
                vcx.update(|window, cx| {
                    window.refresh();
                    let _ = window.draw(cx);
                });
            }
            assert!(
                (position(vcx) - before).abs() < 1.,
                "collapse moved the change in {mode:?}"
            );
            app.update(vcx, |app, cx| {
                app.change_diff_context("context.rs".into(), Some(2..22), cx)
            });
            vcx.run_until_parked();
            app.update(vcx, |app, _| {
                let o = app.tabs[0].diff_overlay.as_ref().unwrap();
                let f = o
                    .file_views
                    .get("context.rs")
                    .unwrap()
                    .shown
                    .as_ref()
                    .unwrap();
                assert_eq!(f.hunks[0].lines[0].new_no, Some(2));
                assert_eq!(o.source, DiffSource::Staged);
            });
            let gap = vcx.debug_bounds("diff-more:context.rs:29:62").unwrap();
            vcx.simulate_click(gap.center(), gpui::Modifiers::none());
            vcx.run_until_parked();
            app.update(vcx, |app, _| {
                let view = &app.tabs[0].diff_overlay.as_ref().unwrap().file_views["context.rs"];
                let shown = view.shown.as_ref().unwrap();
                assert_eq!(
                    shown.hunks.len(),
                    1,
                    "the combined control reveals the entire gap"
                );
                assert_eq!(
                    shown.hunks[0]
                        .lines
                        .iter()
                        .filter(|l| l.kind == LineKind::Added)
                        .count(),
                    2
                );
                assert_eq!(
                    shown.hunks[0]
                        .lines
                        .iter()
                        .filter(|l| l.kind == LineKind::Removed)
                        .count(),
                    2
                );
            });
            app.update_in(vcx, |app, window, cx| {
                app.open_diff_overlay(
                    crate::ui::host_ops::HostId::LOCAL,
                    PathBuf::from("/fixture"),
                    DiffSource::Staged,
                    Some("other.rs".into()),
                    window,
                    cx,
                );
                assert!(
                    app.tabs[0]
                        .diff_overlay
                        .as_ref()
                        .unwrap()
                        .file_views
                        .is_empty(),
                    "switching files must invalidate a pending context reply"
                );
            });
            vcx.update(|window, _| window.remove_window());
        }
    }

    #[gpui::test]
    fn diff_character_selection_copies_partial_lines_in_both_views(cx: &mut TestAppContext) {
        let (app, _) = test_window::harness(cx);
        let mut failures = Vec::new();
        for (mode, selector, prefix) in [
            (DiffViewMode::Split, "diff-old-text", "− "),
            (DiffViewMode::Split, "diff-new-text", "+ "),
            (DiffViewMode::Unified, "diff-unified-text", ""),
        ] {
            let mut file = git_diff::synthesize_added(
                "selection.txt",
                b"before selected after\n",
                &git_diff::DiffBudget::SINGLE_FILE,
            );
            file.hunks[0].lines.insert(
                0,
                git_diff::DiffLine {
                    kind: LineKind::Removed,
                    old_no: Some(1),
                    new_no: None,
                    text: "before selected after".into(),
                },
            );
            let (_, vcx) = cx.add_window_view(|_, _| LongDiff {
                app: app.clone(),
                file,
                mode,
                vertical: gpui::ScrollHandle::new(),
            });
            vcx.run_until_parked();
            vcx.update(|_, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string("UNCHANGED".into()))
            });
            let bounds = vcx.debug_bounds(selector).expect("visible diff text");
            let (start_x, end_x) = vcx.update(|window, cx| {
                let font = gpui::Font {
                    family: app.read(cx).font_family.clone().into(),
                    ..Default::default()
                };
                let size = window.rem_size().as_f32() * 0.75;
                let width = |text: &str| {
                    crate::ui::tab_strip::measure_text(window.text_system(), &font, size, text)
                };
                (
                    width(&format!("{prefix}before ")),
                    width(&format!("{prefix}before selected")),
                )
            });
            let start = point(bounds.left() + px(start_x), bounds.center().y);
            let end = point(bounds.left() + px(end_x), bounds.center().y);
            vcx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.simulate_mouse_move(end, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.run_until_parked();
            vcx.simulate_keystrokes("secondary-c");
            let copied = vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
            if copied.as_deref() != Some("selected") {
                failures.push(format!("{selector}: {copied:?}"));
            }
        }
        assert!(
            failures.is_empty(),
            "partial diff selection failed: {failures:?}"
        );
    }

    #[gpui::test]
    fn diff_file_names_support_selection_and_copy(cx: &mut TestAppContext) {
        let (app, _) = test_window::harness(cx);
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            let path = "src/目录/selected.rs";
            let file = git_diff::synthesize_added(
                path,
                b"body only\n",
                &git_diff::DiffBudget::SINGLE_FILE,
            );
            let (_, vcx) = cx.add_window_view(|_, _| LongDiff {
                app: app.clone(),
                file,
                mode,
                vertical: gpui::ScrollHandle::new(),
            });
            vcx.run_until_parked();
            vcx.update(|_, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string("UNCHANGED".into()));
            });
            let bounds = vcx.debug_bounds("diff-file-name:0").expect("file name");
            let (start_x, end_x) = vcx.update(|window, cx| {
                let font = gpui::Font {
                    family: app.read(cx).font_family.clone().into(),
                    ..Default::default()
                };
                let size = window.rem_size().as_f32() * 0.75;
                let width = |text: &str| {
                    crate::ui::tab_strip::measure_text(window.text_system(), &font, size, text)
                };
                (width("src/目录/"), width(path))
            });
            let start = point(bounds.left() + px(start_x), bounds.center().y);
            let end = point(bounds.left() + px(end_x), bounds.center().y);
            vcx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.simulate_mouse_move(end, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.run_until_parked();
            vcx.simulate_keystrokes("secondary-c");
            assert_eq!(
                vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
                Some("selected.rs".into()),
                "dragging a file name must copy its selected characters in {mode:?}"
            );
            vcx.simulate_keystrokes("secondary-a secondary-c");
            assert_eq!(
                vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
                Some(path.into()),
                "file-name Select All must include the full path"
            );
            vcx.update(|_, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string("UNCHANGED".into()));
            });
            vcx.simulate_mouse_down(start, gpui::MouseButton::Right, gpui::Modifiers::none());
            vcx.simulate_mouse_up(start, gpui::MouseButton::Right, gpui::Modifiers::none());
            vcx.run_until_parked();
            vcx.simulate_keystrokes("down enter");
            assert_eq!(
                vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
                Some(path.into()),
                "the file-name context menu must copy the selected path"
            );
            let body_selector = match mode {
                DiffViewMode::Split => "diff-new-text",
                DiffViewMode::Unified => "diff-unified-text",
            };
            let body = vcx.debug_bounds(body_selector).unwrap().center();
            vcx.simulate_click(body, gpui::Modifiers::none());
            vcx.simulate_keystrokes("secondary-a secondary-c");
            assert_eq!(
                vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
                Some("body only".into()),
                "content copying must stay independent of the file name"
            );
        }
    }

    #[gpui::test]
    fn selecting_a_diff_file_name_preserves_expansion(cx: &mut TestAppContext) {
        let (app, mut initial_window) = test_window::harness(cx);
        initial_window.update(|window, _| window.remove_window());
        let (_, vcx) = cx.add_window_view(|_, _| ContextDiff(app.clone()));
        vcx.simulate_resize(size(px(1000.), px(600.)));
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
                commit_split: crate::ui::diff_overlay::CommitSplit::new(cx),
                document_header_focus: cx.focus_handle(),
                focus_handle: cx.focus_handle(),
                load: DiffLoad::Ready(Arc::new(DiffSnapshot {
                    files: vec![git_diff::synthesize_added(
                        "a.rs",
                        b"body only\n",
                        &git_diff::DiffBudget::SINGLE_FILE,
                    )],
                    ..Default::default()
                })),
                loading: false,
                expanded: HashMap::from([("a.rs".into(), true)]),
                focus: None,
                preview: None,
                preview_loading: None,
                scroll: gpui::ScrollHandle::new(),
                epoch: None,
                file_views: HashMap::new(),
                geometry: Rc::default(),
                anchor: Rc::default(),
            });
            cx.notify();
        });
        vcx.run_until_parked();
        let name = vcx.debug_bounds("diff-file-name:0").unwrap();
        let start = point(name.left() + px(1.), name.center().y);
        let end = point(name.left() + px(60.), name.center().y);
        vcx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::none());
        vcx.simulate_mouse_move(end, gpui::MouseButton::Left, gpui::Modifiers::none());
        vcx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::none());
        vcx.run_until_parked();
        vcx.simulate_keystrokes("secondary-a secondary-c");
        assert_eq!(
            vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("a.rs".into())
        );
        assert!(app.read_with(vcx, |app, _| {
            app.tabs[app.active].diff_overlay.as_ref().unwrap().expanded["a.rs"]
        }));

        let header = vcx.debug_bounds("diff-file-header:0").unwrap();
        vcx.simulate_click(
            point(header.left() + px(17.), header.center().y),
            gpui::Modifiers::none(),
        );
        vcx.run_until_parked();
        assert!(!app.read_with(vcx, |app, _| {
            app.tabs[app.active].diff_overlay.as_ref().unwrap().expanded["a.rs"]
        }));
        let name = vcx.debug_bounds("diff-file-name:0").unwrap();
        vcx.simulate_click(name.center(), gpui::Modifiers::none());
        vcx.simulate_keystrokes("secondary-a secondary-c");
        assert_eq!(
            vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("a.rs".into()),
            "collapsed file names must remain selectable"
        );
        assert!(!app.read_with(vcx, |app, _| {
            app.tabs[app.active].diff_overlay.as_ref().unwrap().expanded["a.rs"]
        }));
    }

    struct LongDiff {
        app: gpui::Entity<Tty7App>,
        file: FileDiff,
        mode: DiffViewMode,
        vertical: gpui::ScrollHandle,
    }

    struct ManyDiffs {
        app: gpui::Entity<Tty7App>,
        snapshot: DiffSnapshot,
        vertical: gpui::ScrollHandle,
    }

    impl gpui::Render for ManyDiffs {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.app.update(cx, |app, cx| {
                div()
                    .w(px(1000.))
                    .h(px(700.))
                    .flex()
                    .child(app.diff_file_list(
                        &self.snapshot,
                        &HashMap::new(),
                        None,
                        &self.vertical,
                        window,
                        cx,
                    ))
            })
        }
    }

    fn many_diffs() -> DiffSnapshot {
        let files = (0..24)
            .map(|file| {
                let text = (0..300)
                    .map(|row| format!("file {file} line {row}: let value = calculate(input);\n"))
                    .collect::<String>();
                git_diff::synthesize_added(
                    &format!("file-{file}.rs"),
                    text.as_bytes(),
                    &git_diff::DiffBudget::PANEL,
                )
            })
            .collect();
        let snapshot = DiffSnapshot {
            files,
            ..Default::default()
        };
        assert!(!snapshot.stats().oversized);
        snapshot
    }

    #[gpui::test]
    fn diff_only_shapes_visible_rows_and_reaches_the_last_file(cx: &mut TestAppContext) {
        use crate::ui::diff_selection::geometry_probe;
        let (app, _) = test_window::harness(cx);
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            cx.update(|cx| cx.global_mut::<Config>().diff_view = mode);
            let vertical = gpui::ScrollHandle::new();
            let (_, vcx) = cx.add_window_view(|_, _| ManyDiffs {
                app: app.clone(),
                snapshot: many_diffs(),
                vertical: vertical.clone(),
            });
            for end in [false, true] {
                if end {
                    vertical.set_offset(point(px(0.), px(-1_000_000.)));
                }
                geometry_probe::reset();
                vcx.update(|window, cx| {
                    window.refresh();
                    let _ = window.draw(cx);
                });
                let shaped = geometry_probe::count();
                assert!(
                    shaped > 0 && shaped < 80,
                    "shaped {shaped} rows in {mode:?}"
                );
                let card = vcx
                    .debug_bounds(if end { "diff-card-23" } else { "diff-card-0" })
                    .unwrap();
                assert!(card.top() < px(700.) && card.bottom() > px(0.));
                if end {
                    let column = if mode == DiffViewMode::Split { 1 } else { 0 };
                    let selector = if column == 1 {
                        "diff-selection-text-1-299"
                    } else {
                        "diff-selection-text-0-299"
                    };
                    assert!(vcx.debug_bounds(selector).is_some(), "last row must render");
                }
            }
        }
    }

    #[gpui::test]
    fn diff_selection_spans_rows_that_scroll_out_of_view(cx: &mut TestAppContext) {
        let (app, _) = test_window::harness(cx);
        let source = (0..200)
            .map(|row| format!("line-{row:03}\n"))
            .collect::<String>();
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            let vertical = gpui::ScrollHandle::new();
            let (_, vcx) = cx.add_window_view(|_, _| LongDiff {
                app: app.clone(),
                mode,
                vertical: vertical.clone(),
                file: git_diff::synthesize_added(
                    "selection.txt",
                    source.as_bytes(),
                    &git_diff::DiffBudget::SINGLE_FILE,
                ),
            });
            let split = mode == DiffViewMode::Split;
            let top = vcx
                .debug_bounds(if split {
                    "diff-selection-text-1-0"
                } else {
                    "diff-selection-text-0-0"
                })
                .unwrap();
            let (a, b) = vcx.update(|window, cx| {
                let font = gpui::Font {
                    family: app.read(cx).font_family.clone().into(),
                    ..Default::default()
                };
                let size = window.rem_size().as_f32() * 0.75;
                let width = |text: &str| {
                    crate::ui::tab_strip::measure_text(window.text_system(), &font, size, text)
                };
                (
                    width(if split { "+ " } else { "" }),
                    width(if split { "+ line-199" } else { "line-199" }),
                )
            });
            let start = point(top.left() + px(a), top.center().y);
            vcx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::none());
            vertical.set_offset(point(px(0.), px(-100_000.)));
            vcx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            let bottom = vcx
                .debug_bounds(if split {
                    "diff-selection-text-1-199"
                } else {
                    "diff-selection-text-0-199"
                })
                .unwrap();
            let end = point(bottom.left() + px(b), bottom.center().y);
            vcx.simulate_mouse_move(end, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.simulate_keystrokes("secondary-c");
            assert_eq!(
                vcx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
                source.trim_end()
            );
        }
    }

    #[gpui::test]
    fn diff_horizontal_offset_survives_scrolling_past_the_file(cx: &mut TestAppContext) {
        let (app, _) = test_window::harness(cx);
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            cx.update(|cx| cx.global_mut::<Config>().diff_view = mode);
            let mut snapshot = many_diffs();
            snapshot.files[0].hunks[0].lines[0].text = format!("{}END", "x".repeat(600));
            let vertical = gpui::ScrollHandle::new();
            let (_, vcx) = cx.add_window_view(|_, _| ManyDiffs {
                app: app.clone(),
                snapshot,
                vertical: vertical.clone(),
            });
            let split = mode == DiffViewMode::Split;
            let viewport = vcx
                .debug_bounds(if split {
                    "diff-new-scroll"
                } else {
                    "diff-unified-scroll"
                })
                .unwrap();
            vcx.simulate_event(gpui::ScrollWheelEvent {
                position: point(viewport.center().x, px(80.)),
                delta: gpui::ScrollDelta::Pixels(point(px(-100_000.), px(0.))),
                ..Default::default()
            });
            vcx.run_until_parked();
            let selector = if split {
                "diff-selection-text-1-0"
            } else {
                "diff-selection-text-0-0"
            };
            let before = vcx.debug_bounds(selector).unwrap();
            assert!(before.left() < viewport.left());
            vertical.set_offset(point(px(0.), px(-1_000_000.)));
            vcx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            vertical.set_offset(point(px(0.), px(0.)));
            vcx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            assert_eq!(
                vcx.debug_bounds(selector).unwrap().left(),
                before.left(),
                "{mode:?}"
            );
        }
    }

    #[gpui::test]
    fn gap_context_controls_stack_in_the_hunk_gutter(cx: &mut TestAppContext) {
        let (app, _) = test_window::harness(cx);
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            let mut file = git_diff::synthesize_added(
                "gutter.rs",
                b"first\n",
                &git_diff::DiffBudget::SINGLE_FILE,
            );
            file.hunks.push(git_diff::Hunk {
                header: "@@ -100,0 +100 @@".into(),
                lines: vec![git_diff::DiffLine {
                    kind: LineKind::Added,
                    old_no: None,
                    new_no: Some(100),
                    text: "last".into(),
                }],
            });
            let (_, vcx) = cx.add_window_view(|_, _| LongDiff {
                app: app.clone(),
                file,
                mode,
                vertical: gpui::ScrollHandle::new(),
            });
            vcx.run_until_parked();

            let up = vcx
                .debug_bounds("diff-more:gutter.rs:80:100")
                .expect("up control is visible");
            let down = vcx
                .debug_bounds("diff-more:gutter.rs:2:22")
                .expect("down control is visible");
            assert_eq!(up.left(), down.left(), "controls share the left gutter");
            assert!(up.top() < down.top(), "up control is stacked above down");
            assert!(up.bottom() <= down.top(), "controls do not overlap");
        }
    }

    #[gpui::test]
    fn short_context_gap_uses_one_full_width_hunk_row(cx: &mut TestAppContext) {
        let (app, _) = test_window::harness(cx);
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            let mut file = git_diff::synthesize_added(
                "short.rs",
                b"first\n",
                &git_diff::DiffBudget::SINGLE_FILE,
            );
            file.hunks.push(git_diff::Hunk {
                header: "@@ -20,0 +20 @@".into(),
                lines: vec![git_diff::DiffLine {
                    kind: LineKind::Added,
                    old_no: None,
                    new_no: Some(20),
                    text: "last".into(),
                }],
            });
            let (_, vcx) = cx.add_window_view(|_, _| LongDiff {
                app: app.clone(),
                file,
                mode,
                vertical: gpui::ScrollHandle::new(),
            });
            vcx.run_until_parked();
            for (viewport, first, gap) in if mode == DiffViewMode::Split {
                vec![
                    (
                        "diff-old-scroll",
                        "diff-hunk-header:0:0",
                        "diff-hunk-header:0:1",
                    ),
                    (
                        "diff-new-scroll",
                        "diff-hunk-header:1:0",
                        "diff-hunk-header:1:1",
                    ),
                ]
            } else {
                vec![(
                    "diff-unified-scroll",
                    "diff-hunk-header:0:0",
                    "diff-hunk-header:0:1",
                )]
            } {
                let viewport = vcx.debug_bounds(viewport).unwrap();
                let first = vcx.debug_bounds(first).unwrap();
                let gap = vcx.debug_bounds(gap).unwrap();
                assert!(
                    (gap.right() - viewport.right()).abs() < px(0.01),
                    "hunk background must fill the column without horizontal overflow in {mode:?}: {gap:?} vs {viewport:?}"
                );
                assert_eq!(
                    gap.size.height, first.size.height,
                    "a short gap uses one combined unfold control in {mode:?}"
                );
            }
            assert!(vcx.debug_bounds("diff-more:short.rs:2:20").is_some());
        }
    }

    #[gpui::test]
    fn short_hunks_keep_the_full_column_width_after_horizontal_scroll(cx: &mut TestAppContext) {
        use crate::ui::diff_selection::geometry_probe;
        let (app, _) = test_window::harness(cx);
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            let mut file = git_diff::synthesize_added(
                "width.txt",
                b"short\n",
                &git_diff::DiffBudget::SINGLE_FILE,
            );
            file.hunks.push(git_diff::Hunk {
                header: "@@ -0,0 +20 @@".into(),
                lines: vec![git_diff::DiffLine {
                    kind: LineKind::Added,
                    old_no: None,
                    new_no: Some(20),
                    text: "x".repeat(600),
                }],
            });
            let (_, vcx) = cx.add_window_view(|_, _| LongDiff {
                app: app.clone(),
                file,
                mode,
                vertical: gpui::ScrollHandle::new(),
            });
            let viewport = vcx
                .debug_bounds(if mode == DiffViewMode::Split {
                    "diff-new-scroll"
                } else {
                    "diff-unified-scroll"
                })
                .unwrap();
            vcx.simulate_event(gpui::ScrollWheelEvent {
                position: viewport.center(),
                delta: gpui::ScrollDelta::Pixels(point(px(-100_000.), px(0.))),
                ..Default::default()
            });
            vcx.run_until_parked();
            geometry_probe::reset();
            vcx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            assert_eq!(
                geometry_probe::count(),
                2,
                "both hunk rows span the column in {mode:?}"
            );
            for selector in if mode == DiffViewMode::Split {
                ["diff-hunk-header:1:0", "diff-hunk-header:1:1"]
            } else {
                ["diff-hunk-header:0:0", "diff-hunk-header:0:1"]
            } {
                let header = vcx.debug_bounds(selector).expect("visible hunk header");
                assert!(
                    header.right() >= viewport.right() - px(1.),
                    "hunk background must fill the scrolled viewport in {mode:?}: {header:?} vs {viewport:?}"
                );
            }
        }
    }

    #[gpui::test]
    #[ignore = "manual diff frame benchmark; run with --ignored --nocapture"]
    fn diff_many_files_frame_profile(cx: &mut TestAppContext) {
        let (app, _) = test_window::harness(cx);
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            cx.update(|cx| cx.global_mut::<Config>().diff_view = mode);
            let snapshot = many_diffs();
            let vertical = gpui::ScrollHandle::new();
            let start = std::time::Instant::now();
            let (_, vcx) = cx.add_window_view(|_, _| ManyDiffs {
                app: app.clone(),
                snapshot,
                vertical: vertical.clone(),
            });
            vcx.run_until_parked();
            let first_ms = start.elapsed().as_secs_f64() * 1000.;
            let mut samples = Vec::new();
            for frame in 0..30 {
                let start = std::time::Instant::now();
                vcx.simulate_event(gpui::ScrollWheelEvent {
                    position: point(px(900.), px(350.)),
                    delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-120.))),
                    ..Default::default()
                });
                vcx.run_until_parked();
                if frame >= 5 {
                    samples.push(start.elapsed().as_secs_f64() * 1000.);
                }
            }
            assert!(
                vertical.offset().y < px(-1000.),
                "profile must scroll the diff"
            );
            samples.sort_by(f64::total_cmp);
            eprintln!(
                "DIFF_FRAME_PROFILE {}",
                serde_json::json!({
                    "mode": format!("{mode:?}"), "files": 24, "lines": 7200,
                    "first_frame_ms": first_ms,
                    "scroll_median_ms": samples[samples.len()/2],
                    "scroll_p95_ms": samples[samples.len()*95/100],
                })
            );
            assert!(
                first_ms < 250.,
                "opening the diff blocks for {first_ms:.1} ms"
            );
        }
    }

    #[gpui::test]
    fn diff_character_selection_copies_unicode_across_lines_and_backwards(cx: &mut TestAppContext) {
        let (app, _) = test_window::harness(cx);
        let first = "before 中👩‍💻\tend";
        let second = "\tsecond after";
        for (mode, column, prefix) in [
            (DiffViewMode::Split, 0, "− "),
            (DiffViewMode::Split, 1, "+ "),
            (DiffViewMode::Unified, 0, ""),
        ] {
            let mut file = git_diff::synthesize_added(
                "selection.txt",
                format!("{first}\n{second}\n").as_bytes(),
                &git_diff::DiffBudget::SINGLE_FILE,
            );
            for (index, text) in [first, second].into_iter().enumerate() {
                file.hunks[0].lines.insert(
                    index,
                    git_diff::DiffLine {
                        kind: LineKind::Removed,
                        old_no: Some(index as u32 + 1),
                        new_no: None,
                        text: text.into(),
                    },
                );
            }
            file.hunks[0].lines.insert(
                2,
                git_diff::DiffLine {
                    kind: LineKind::Removed,
                    old_no: Some(3),
                    new_no: None,
                    text: "obsolete padding".into(),
                },
            );
            file.hunks.push(git_diff::Hunk {
                header: "@@ -4 +3 @@".into(),
                lines: vec![git_diff::DiffLine {
                    kind: LineKind::Context,
                    old_no: Some(4),
                    new_no: Some(3),
                    text: "tail".into(),
                }],
            });
            let (_, vcx) = cx.add_window_view(|_, _| LongDiff {
                app: app.clone(),
                file,
                mode,
                vertical: gpui::ScrollHandle::new(),
            });
            vcx.run_until_parked();
            let a = vcx
                .debug_bounds(if column == 0 {
                    "diff-selection-text-0-0"
                } else {
                    "diff-selection-text-1-0"
                })
                .unwrap();
            let b = vcx
                .debug_bounds(if column == 0 {
                    "diff-selection-text-0-1"
                } else {
                    "diff-selection-text-1-1"
                })
                .unwrap();
            let (x, y) = vcx.update(|window, cx| {
                let font = gpui::Font {
                    family: app.read(cx).font_family.clone().into(),
                    ..Default::default()
                };
                let size = window.rem_size().as_f32() * 0.75;
                let width = |text: &str| {
                    crate::ui::tab_strip::measure_text(window.text_system(), &font, size, text)
                };
                (
                    width(&format!("{prefix}before ")),
                    width(&format!("{prefix}    second")),
                )
            });
            let start = point(a.left() + px(x), a.center().y);
            let end = point(b.left() + px(y), b.center().y);
            for (a, b) in [(start, end), (end, start)] {
                vcx.simulate_mouse_down(a, gpui::MouseButton::Left, gpui::Modifiers::none());
                vcx.simulate_mouse_move(b, gpui::MouseButton::Left, gpui::Modifiers::none());
                vcx.simulate_mouse_up(b, gpui::MouseButton::Left, gpui::Modifiers::none());
                vcx.run_until_parked();
                vcx.simulate_keystrokes("secondary-c");
                let text = vcx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
                assert_eq!(text, "中👩‍💻\tend\n\tsecond", "{mode:?} column {column}");
            }
            vcx.simulate_click(start, gpui::Modifiers::none());
            vcx.simulate_keystrokes("shift-right shift-right secondary-c");
            let text = vcx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
            assert_eq!(
                text, "中👩‍💻",
                "keyboard selection must keep graphemes intact"
            );
            if mode == DiffViewMode::Split && column == 1 {
                let tail = vcx.debug_bounds("diff-selection-text-1-2").unwrap();
                let width = vcx.update(|window, cx| {
                    let font = gpui::Font {
                        family: app.read(cx).font_family.clone().into(),
                        ..Default::default()
                    };
                    crate::ui::tab_strip::measure_text(
                        window.text_system(),
                        &font,
                        window.rem_size().as_f32() * 0.75,
                        "  tail",
                    )
                });
                let tail = point(tail.left() + px(width), tail.center().y);
                vcx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::none());
                vcx.simulate_mouse_move(tail, gpui::MouseButton::Left, gpui::Modifiers::none());
                vcx.simulate_mouse_up(tail, gpui::MouseButton::Left, gpui::Modifiers::none());
                vcx.simulate_keystrokes("secondary-c");
                assert_eq!(
                    vcx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
                    "中👩‍💻\tend\n\tsecond after\ntail",
                    "padding and hunk headers are not source text"
                );
            }
        }
    }

    impl gpui::Render for LongDiff {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let card = self.app.update(cx, |app, cx| {
                app.diff_file_card(0, &self.file, true, self.mode, window, cx)
            });
            div()
                .id("test-diff-scroll")
                .w(window.viewport_size().width.min(px(600.)))
                .h(px(250.))
                .overflow_y_scroll()
                .track_scroll(&self.vertical)
                .child(card)
        }
    }

    #[gpui::test]
    fn diff_character_selection_tracks_horizontal_scroll_and_edge_drag(cx: &mut TestAppContext) {
        let (app, _) = test_window::harness(cx);
        let text = format!("{}END", "x".repeat(300));
        for (mode, column, viewport, prefix) in [
            (DiffViewMode::Split, 0, "diff-old-scroll", "− "),
            (DiffViewMode::Split, 1, "diff-new-scroll", "+ "),
            (DiffViewMode::Unified, 0, "diff-unified-scroll", ""),
        ] {
            let mut file = git_diff::synthesize_added(
                "long.txt",
                text.as_bytes(),
                &git_diff::DiffBudget::SINGLE_FILE,
            );
            file.hunks[0].lines.insert(
                0,
                git_diff::DiffLine {
                    kind: LineKind::Removed,
                    old_no: Some(1),
                    new_no: None,
                    text: text.clone(),
                },
            );
            let (_, vcx) = cx.add_window_view(|_, _| LongDiff {
                app: app.clone(),
                file,
                mode,
                vertical: gpui::ScrollHandle::new(),
            });
            vcx.run_until_parked();
            let viewport = vcx.debug_bounds(viewport).unwrap();
            vcx.simulate_event(gpui::ScrollWheelEvent {
                position: viewport.center(),
                delta: gpui::ScrollDelta::Pixels(point(px(-100_000.), px(0.))),
                ..Default::default()
            });
            vcx.run_until_parked();
            let selector = if column == 0 {
                "diff-selection-text-0-0"
            } else {
                "diff-selection-text-1-0"
            };
            let bounds = vcx.debug_bounds(selector).unwrap();
            let (a, b) = vcx.update(|window, cx| {
                let font = gpui::Font {
                    family: app.read(cx).font_family.clone().into(),
                    ..Default::default()
                };
                let size = window.rem_size().as_f32() * 0.75;
                let width = |text: &str| {
                    crate::ui::tab_strip::measure_text(window.text_system(), &font, size, text)
                };
                (
                    width(&format!("{prefix}{}", "x".repeat(300))),
                    width(&format!("{prefix}{text}")),
                )
            });
            let start = point(bounds.left() + px(a), bounds.center().y);
            let end = point(bounds.left() + px(b), bounds.center().y);
            vcx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.simulate_mouse_move(end, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.simulate_keystrokes("secondary-c");
            assert_eq!(
                vcx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
                "END"
            );
            vcx.simulate_mouse_down(end, gpui::MouseButton::Left, gpui::Modifiers::none());
            let outside = point(viewport.left() - px(30.), end.y);
            vcx.simulate_mouse_move(outside, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.run_until_parked();
            vcx.executor()
                .advance_clock(std::time::Duration::from_millis(32));
            vcx.run_until_parked();
            assert!(
                vcx.debug_bounds(selector).unwrap().left() > bounds.left(),
                "edge selection must scroll back along a long line"
            );
            vcx.simulate_mouse_up(outside, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.simulate_keystrokes("secondary-c");
            let copied = vcx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
            assert!(
                copied.starts_with('x') && copied.ends_with("END"),
                "{copied:?}"
            );
        }
    }

    #[gpui::test]
    fn long_diff_lines_can_scroll_to_the_end_in_both_views(cx: &mut TestAppContext) {
        let (app, _) = test_window::harness(cx);
        let text = format!("\t{}END", "长文字 with spaces ".repeat(80));
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            let mut file = git_diff::synthesize_added(
                "long.txt",
                text.as_bytes(),
                &git_diff::DiffBudget::SINGLE_FILE,
            );
            file.hunks[0].lines.insert(
                0,
                git_diff::DiffLine {
                    kind: LineKind::Removed,
                    old_no: Some(1),
                    new_no: None,
                    text: text.clone(),
                },
            );
            let (_, vcx) = cx.add_window_view(|_, _| LongDiff {
                app: app.clone(),
                file,
                mode,
                vertical: gpui::ScrollHandle::new(),
            });
            vcx.run_until_parked();
            assert!(
                vcx.debug_bounds("diff-horizontal-scrollbar").is_some(),
                "long lines need a horizontal scrollbar in {mode:?}",
            );
            let columns: &[(&str, &str)] = match mode {
                DiffViewMode::Split => &[
                    ("diff-old-scroll", "diff-old-text"),
                    ("diff-new-scroll", "diff-new-text"),
                ],
                DiffViewMode::Unified => &[("diff-unified-scroll", "diff-unified-text")],
            };
            for &(viewport, text) in columns {
                let viewport = vcx.debug_bounds(viewport).expect("scroll viewport");
                let before = vcx.debug_bounds(text).expect("long text");
                assert!(before.right() > viewport.right(), "fixture must overflow");
                vcx.simulate_event(gpui::ScrollWheelEvent {
                    position: viewport.center(),
                    delta: gpui::ScrollDelta::Pixels(point(px(-100_000.), px(0.))),
                    ..Default::default()
                });
                vcx.run_until_parked();
                let end = vcx.debug_bounds(text).expect("scrolled text");
                assert!(
                    end.left() < before.left(),
                    "horizontal wheel must move text"
                );
                assert!(
                    end.right() <= viewport.right() + px(1.),
                    "line ending must be reachable in {mode:?}"
                );
                vcx.simulate_event(gpui::ScrollWheelEvent {
                    position: viewport.center(),
                    delta: gpui::ScrollDelta::Pixels(point(px(80.), px(0.))),
                    ..Default::default()
                });
                vcx.run_until_parked();
                assert!(
                    vcx.debug_bounds(text).unwrap().left() > end.left(),
                    "horizontal wheel must return from the line ending"
                );
                vcx.simulate_event(gpui::ScrollWheelEvent {
                    position: viewport.center(),
                    delta: gpui::ScrollDelta::Pixels(point(px(-80.), px(0.))),
                    ..Default::default()
                });
                vcx.run_until_parked();
                vcx.simulate_event(gpui::ScrollWheelEvent {
                    position: viewport.center(),
                    delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-40.))),
                    ..Default::default()
                });
                vcx.run_until_parked();
                assert_eq!(
                    vcx.debug_bounds(text).unwrap().left(),
                    end.left(),
                    "vertical wheel must not scroll sideways"
                );
            }
            let &(viewport_selector, text_selector) = columns.last().unwrap();
            let bar = vcx.debug_bounds("diff-horizontal-scrollbar").unwrap();
            let start = point(bar.left() + px(1.), bar.center().y);
            vcx.simulate_click(start, gpui::Modifiers::none());
            vcx.run_until_parked();
            let start_text = vcx.debug_bounds(text_selector).unwrap();
            let viewport = vcx.debug_bounds(viewport_selector).unwrap();
            assert!(
                start_text.left() >= viewport.left(),
                "scrollbar track click returns to the start"
            );
            let thumb = point(bar.left() + px(20.), bar.center().y);
            let end = point(bar.right() - px(1.), bar.center().y);
            vcx.simulate_mouse_down(thumb, gpui::MouseButton::Left, gpui::Modifiers::none());
            // The shared scrollbar throttles drag updates using a real clock.
            std::thread::sleep(std::time::Duration::from_millis(30));
            vcx.simulate_mouse_move(end, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::none());
            vcx.run_until_parked();
            assert!(
                vcx.debug_bounds(text_selector).unwrap().right() <= viewport.right() + px(1.),
                "dragging the horizontal scrollbar reaches the line ending"
            );
        }
    }

    #[gpui::test]
    fn horizontal_diff_viewports_leave_vertical_wheel_to_the_file_list(cx: &mut TestAppContext) {
        let (app, _) = test_window::harness(cx);
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            let vertical = gpui::ScrollHandle::new();
            let text = format!("{}\n", "long text ".repeat(100)).repeat(40);
            let (_, vcx) = cx.add_window_view(|_, _| LongDiff {
                app: app.clone(),
                file: git_diff::synthesize_added(
                    "long.txt",
                    text.as_bytes(),
                    &git_diff::DiffBudget::SINGLE_FILE,
                ),
                mode,
                vertical: vertical.clone(),
            });
            vcx.run_until_parked();
            let selector = match mode {
                DiffViewMode::Split => "diff-new-scroll",
                DiffViewMode::Unified => "diff-unified-scroll",
            };
            let viewport = vcx.debug_bounds(selector).unwrap();
            vcx.simulate_event(gpui::ScrollWheelEvent {
                position: point(viewport.center().x, px(80.)),
                delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-40.))),
                ..Default::default()
            });
            vcx.run_until_parked();
            assert!(
                vertical.offset().y < px(0.),
                "the file list must still scroll vertically in {mode:?}"
            );
        }
    }

    // Independent expectations from the approved GitHub reference colors.
    #[gpui::test]
    fn rendered_changes_use_github_colors_in_both_modes(cx: &mut TestAppContext) {
        let (app, mut vcx) = test_window::harness(cx);
        for (mode, added, added_bg, removed, removed_bg) in [
            (ThemeMode::Light, 0x116329, 0xdafbe1, 0x82071e, 0xffebe9),
            (ThemeMode::Dark, 0xaff5b4, 0x033a16, 0xffdcd7, 0x67060c),
        ] {
            vcx.update(|window, cx| Theme::change(mode, Some(window), cx));
            for (kind, side, fg, bg) in [
                (LineKind::Added, Side::New, added, added_bg),
                (LineKind::Removed, Side::Old, removed, removed_bg),
            ] {
                let expected_fg = gpui::Hsla::from(gpui::rgb(fg));
                let expected_bg = gpui::Fill::from(gpui::rgb(bg));
                let mut unified = app.update_in(&mut vcx, |app, _, cx| {
                    app.diff_unified_row(
                        &UnifiedRow {
                            kind,
                            old: None,
                            new: Some(2),
                            text: "changed".into(),
                        },
                        false,
                        cx,
                    )
                });
                assert_eq!(unified.style().background, Some(expected_bg.clone()));
                assert_eq!(unified.style().text.color, Some(expected_fg));
                vcx.draw(point(px(0.), px(0.)), size(px(700.), px(19.)), |_, _| {
                    unified
                });
                let mut split = app.update_in(&mut vcx, |app, _, cx| {
                    app.diff_split_cell(
                        Some(&SplitCell {
                            no: Some(2),
                            text: "changed".into(),
                            changed: true,
                        }),
                        side,
                        px(0.),
                        cx,
                    )
                });
                assert_eq!(split.style().background, Some(expected_bg));
                assert_eq!(split.style().text.color, Some(expected_fg));
                vcx.draw(point(px(0.), px(0.)), size(px(350.), px(19.)), |_, _| split);
            }
        }
    }
    #[gpui::test]
    fn rendered_context_and_file_cards_share_the_application_surface(cx: &mut TestAppContext) {
        let (app, mut vcx) = test_window::harness(cx);
        for (preset, bg) in [("light", 0xffffff), ("dark", 0x000000), ("nord", 0x2e3440)] {
            let (expected_fg, border) = app.update_in(&mut vcx, |app, window, cx| {
                app.set_preset(preset, window, cx);
                // Unrelated custom-preset status colors must not affect reading.
                let theme = Theme::global_mut(cx);
                theme.success = gpui::rgb(0xff00ff).into();
                theme.danger = gpui::rgb(0x00ffff).into();
                (cx.theme().foreground, cx.theme().border)
            });
            let expected_bg = gpui::Fill::from(gpui::rgb(bg));
            let mut context = app.update_in(&mut vcx, |app, _, cx| {
                app.diff_unified_row(
                    &UnifiedRow {
                        kind: LineKind::Context,
                        old: Some(1),
                        new: Some(1),
                        text: "keep".into(),
                    },
                    false,
                    cx,
                )
            });
            assert_eq!(context.style().background, Some(expected_bg.clone()));
            assert_eq!(context.style().text.color, Some(expected_fg));
            vcx.draw(point(px(0.), px(0.)), size(px(700.), px(19.)), |_, _| {
                context
            });
            let file =
                git_diff::synthesize_added("new.txt", b"new\n", &git_diff::DiffBudget::SINGLE_FILE);
            for mode in [DiffViewMode::Unified, DiffViewMode::Split] {
                let mut card = app.update_in(&mut vcx, |app, window, cx| {
                    app.diff_file_card(0, &file, false, mode, window, cx)
                });
                assert_eq!(card.style().background, Some(expected_bg.clone()));
                assert_eq!(card.style().text.color, Some(expected_fg));
                assert_eq!(card.style().border_color, Some(border));
                // Inspect the card shell here. Expanded scroll state needs a
                // rendering view, exercised by the window tests above.
            }
        }
    }
}
