use gpui::{
    Animation, AnimationExt as _, AnyElement, Axis, Bounds, ClickEvent, Context, Div, FontWeight,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, SharedString, Stateful,
    Window, canvas, deferred, div, ease_out_quint, linear_color_stop, linear_gradient, point,
    prelude::*, px, size,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::menu::{
    ContextMenu, ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem,
};
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use std::path::{Path, PathBuf};

use crate::core::config::{Config, SidebarGrouping};
use crate::daemon::protocol::RemoteKind;
use crate::terminal::git_status::GitStatusCache;
use crate::ui::app::{TITLE_BAR_HEIGHT, Tty7App};
use crate::ui::hints::tab_badge_label;
use crate::ui::i18n::{L10nKey, t, t_fmt};
use crate::ui::reorder::{self, Reorder, Surface};
use crate::ui::right_panel::RESIZE_HANDLE_WIDTH;
use crate::ui::tab_strip::{
    DragTab, REORDER_SLIDE_MS, TabAvatar, abbreviate_home, elide_keep_edges, elide_label,
    elide_path_keep_tail, measure_text,
};

pub(crate) const MIN_SIDEBAR_WIDTH: f32 = 180.;

const GRAB_HANDLE_W: f32 = 48.;

const ROW_GAP: f32 = 2.;

// Match workspace_head: 5px outer inset + 6px button padding + half its 20px avatar.
const LEADING_ICON_CENTER: f32 = 21.;

/// The row chrome the text budget has to be measured around. These are the
/// numbers the layout below is built from, not a second guess at it — a row
/// that elides against a budget wider than it really has falls back to CSS
/// truncation, which drops the tail this whole module exists to keep.
mod row_metrics {
    /// `border_r_1` on the sidebar itself.
    pub(super) const BORDER: f32 = 1.;
    /// `px_1` on the scrolling list that holds the rows.
    pub(super) const LIST_PAD: f32 = 4.;
    /// `pl_2` + `pr_2` on the row.
    pub(super) const ROW_PAD: f32 = 8.;
    /// Extra space before rows that sit under a group header.
    pub(super) const GROUP_INDENT: f32 = 12.;
    /// The avatar handed to `tab_avatar`.
    pub(super) const AVATAR: f32 = 22.;
    /// `gap_2` between the row's children.
    pub(super) const GAP: f32 = 8.;
    /// The ⌘N badge, when one is shown.
    pub(super) const BADGE: f32 = 20.;
    /// `gap_1p5`, between the branch icon and its text and before the counts.
    pub(super) const META_GAP: f32 = 6.;
    /// The branch icon.
    pub(super) const BRANCH_ICON: f32 = 11.;
    /// Tab title size in rem (13px on a 16px rem).
    pub(super) const TITLE_REM: f32 = 0.8125;
    /// Status tags use smaller type than the metadata, at semibold weight.
    pub(super) const STATUS_REM: f32 = 0.6875;
    /// Horizontal padding on each side of the status tag.
    pub(super) const STATUS_PAD: f32 = 5.;

    /// What a row can spend on text, before the badge is taken out.
    pub(super) const fn text_budget(width: f32) -> f32 {
        width - BORDER - 2. * LIST_PAD - 2. * ROW_PAD - AVATAR - GAP
    }
}

/// What a sidebar row rendered, next to what it had to leave out, so the
/// hover card can be built by comparison instead of deriving the same strings
/// a second time — the two derivations have to agree, and the shortest way to
/// guarantee that is to only ever have one.
struct SidebarRowShown {
    /// The elided title, and the full string it came from. `None` when the
    /// row is showing a placeholder (`Shell 3`) rather than a real title,
    /// which nothing can expand.
    title: Option<(SharedString, SharedString)>,
    branch: Option<(SharedString, SharedString, u32, u32)>,
    cwd: Option<(SharedString, SharedString)>,
}

#[derive(Clone)]
pub(crate) struct DragGroup;

impl Render for DragGroup {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Every detail a sidebar row could not fit, collected so the hover card can
/// be rendered from cloneable data (an `AnyElement` cannot be cloned, but the
/// tooltip closure has to rebuild its content on every hover).
#[derive(Clone)]
struct SidebarInfo {
    /// Full path, when the row's title was elided.
    title: Option<SharedString>,
    /// Full branch plus diff counts, when the row's branch was elided.
    branch: Option<(SharedString, u32, u32)>,
    /// Full working directory, when the row's second line was elided.
    cwd: Option<SharedString>,
    /// Remote host, which the row itself never names.
    host: Option<SharedString>,
}

impl Tty7App {
    /// Whether the tab rail is on screen — the same three conditions `render`
    /// assembles the layout from, in one place the panel opposite can ask.
    pub(crate) fn sidebar_open(&self, cx: &gpui::App) -> bool {
        cx.global::<Config>().tab_bar_position == crate::core::config::TabBarPosition::Left
            && !self.tabs.is_empty()
            && !self.sidebar_collapsed
    }

    /// What the right panel has reserved, from the sidebar's point of view.
    pub(crate) fn right_panel_floor(&self, cx: &gpui::App) -> f32 {
        if self.right_panel_open(cx) {
            crate::ui::right_panel::MIN_WIDTH
        } else {
            0.
        }
    }

    pub(crate) fn sidebar_max_px(&self, window: &Window, cx: &gpui::App) -> f32 {
        crate::ui::app::side_panel_max(
            window.viewport_size().width.as_f32(),
            MIN_SIDEBAR_WIDTH,
            self.right_panel_floor(cx) + self.document_floor(cx),
        )
    }

    /// How wide the sidebar is drawn, given the live cell and the cap the rest
    /// of the window leaves it. Read here rather than clamped at each caller so
    /// the document column's budget and the sidebar itself can never disagree
    /// about how much width is already spoken for.
    pub(crate) fn sidebar_px(&self, window: &Window, cx: &gpui::App) -> f32 {
        self.sidebar_width
            .get()
            .clamp(MIN_SIDEBAR_WIDTH, self.sidebar_max_px(window, cx))
    }

    pub(crate) fn tab_sidebar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let active = self.active;
        let sf = cx.global::<crate::ui::presets::Surfaces>().sidebar;
        let show_badges = self.mod_hint_badges;
        let width = self.sidebar_px(window, cx);
        let query = self.sidebar_search.read(cx).value().trim().to_lowercase();
        // Blanked here, written again from paint: a row filtered out by the
        // search — or hidden with its collapsed group — must leave no rectangle
        // behind for a pane to be dropped between.
        *self.sidebar_slots.borrow_mut() = vec![Bounds::default(); self.tabs.len()];
        // Every group drops itself when its rows filter out, so a query that
        // matches nothing left the sidebar showing only its own search box.
        let mut any_rows = false;

        let mut list = v_flex()
            .id("tab-sidebar-list")
            .track_scroll(&self.sidebar_scroll)
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .px_1()
            .py_1p5()
            .gap_0p5();

        let keys: Rc<Vec<Option<PathBuf>>> = Rc::new(self.sidebar_group_keys(cx));
        // Fold updates save config. Doing that here re-enters render, so the
        // reveal waits until this frame has finished painting.
        self.schedule_path_open_reveal(window, cx);
        let is_local = self.can_spawn_locally(cx);
        let pinned_cfg: Rc<Vec<String>> = Rc::new(self.sidebar_pinned_groups(cx).to_vec());
        if is_local {
            self.probe_pinned_dirs(&keys, &pinned_cfg, cx);
        }
        let keep_empty =
            |p: &Path| is_local && self.pinned_dir_cache.borrow().get(p) == Some(&true);
        let sections = self.sidebar_sections(&keys, cx);
        let headed_keys: Vec<String> = sections
            .iter()
            .filter(|s| s.name.is_some())
            .map(|s| group_name_key(&s.key))
            .collect();
        let all_collapsed = !headed_keys.is_empty()
            && headed_keys
                .iter()
                .all(|k| cx.global::<Config>().sidebar_collapsed_groups.contains(k));

        // ⌘N runs ActivateTabN, which goes through `activate_visual` — the
        // Nth row as the sidebar lays it out, not the Nth tab in `self.tabs`.
        // The badge has to be read off the same order or it names a chord that
        // opens a different tab, so take it from `visual_tab_order` rather than
        // flattening `sections` a second time here.
        let badge_pos: Vec<usize> = {
            let mut pos = vec![0usize; self.tabs.len()];
            for (n, i) in self.visual_tab_order(cx).into_iter().enumerate() {
                pos[i] = n;
            }
            pos
        };

        // The row shows an elided title and a branch; the filter used to read
        // only the elided title, so typing the branch you can see, or the part
        // of the path the row dropped, matched nothing. The label is built
        // here only when there is a query to match it against — the rows
        // themselves elide against measured width and no longer need it.
        let visible_by_section: Vec<Vec<usize>> = sections
            .iter()
            .map(|s| {
                s.tabs
                    .iter()
                    .copied()
                    .filter(|&i| {
                        query.is_empty()
                            || self
                                .tab_label(&self.tabs[i], i, Some(window), cx)
                                .to_lowercase()
                                .contains(&query)
                            || self.tabs[i]
                                .leaf_title(Some(window), cx)
                                .to_lowercase()
                                .contains(&query)
                            || self.tabs[i]
                                .title_details(Some(window), cx)
                                .context
                                .is_some_and(|context| context.to_lowercase().contains(&query))
                            || self.tabs[i]
                                .git_status(Some(window), cx)
                                .is_some_and(|g| g.branch.to_lowercase().contains(&query))
                    })
                    .collect()
            })
            .collect();

        let pointer = window.mouse_position();
        // The row text is measured against real glyphs before it is elided:
        // tab title uses `TITLE_REM` (0.8125rem / 13px) and `text_xs` 0.75rem,
        // resolved here so the measurement and the render use the same sizes and family.
        let font = gpui::Font {
            family: cx.theme().font_family.clone(),
            features: Default::default(),
            fallbacks: None,
            weight: Default::default(),
            style: Default::default(),
        };
        // The active row renders its title at `FontWeight::MEDIUM`, which is
        // wider than the regular weight in any proportional face. Measuring
        // it as regular would let the one row the user is looking at overflow
        // into the truncation this is here to avoid.
        let title_font_active = gpui::Font {
            weight: FontWeight::MEDIUM,
            ..font.clone()
        };
        let status_font = gpui::Font {
            weight: FontWeight::SEMIBOLD,
            ..font.clone()
        };
        let dark_sidebar = crate::ui::presets::surface_is_dark(cx.theme().sidebar);
        let rem = window.rem_size().as_f32();
        let rendered = |ix: &usize| {
            !visible_by_section[*ix].is_empty()
                || (sections[*ix].pinned && sections[*ix].tabs.is_empty() && query.is_empty())
        };
        let pinned_slots: Vec<usize> = (0..sections.len())
            .filter(|&ix| sections[ix].key.is_some() && sections[ix].pinned)
            .filter(rendered)
            .collect();
        let pinned_groups = pinned_slots.len();
        let pinned_group_slots: Rc<RefCell<Vec<Bounds<Pixels>>>> =
            Rc::new(RefCell::new(vec![Bounds::default(); pinned_groups]));
        let pinned_bounds_seen: Rc<RefCell<Vec<bool>>> =
            Rc::new(RefCell::new(vec![false; pinned_groups]));
        let pinned_preview = reorder::preview(
            &self.reorder,
            &Surface::PinnedGroups,
            pinned_groups,
            pointer,
        );
        let pinned_roots: Vec<PathBuf> = pinned_slots
            .iter()
            .filter_map(|&ix| sections[ix].key.clone())
            .collect();
        let pinned_slot_display: Vec<usize> = match &pinned_preview {
            Some(p) => {
                if let (Some(from), Some(to)) =
                    (pinned_roots.get(p.from), pinned_roots.get(p.target))
                    && let Some(order) = reordered_pinned(&pinned_cfg, from, to)
                {
                    reorder::set_pending_pinned(&self.reorder, order);
                }
                p.order.clone()
            }
            None => (0..pinned_groups).collect(),
        };

        let repo_slots: Vec<usize> = (0..sections.len())
            .filter(|&ix| sections[ix].key.is_some() && !sections[ix].pinned)
            .filter(rendered)
            .collect();
        let repo_groups = repo_slots.len();
        let group_slots: Rc<RefCell<Vec<Bounds<Pixels>>>> =
            Rc::new(RefCell::new(vec![Bounds::default(); repo_groups]));
        let group_bounds_seen: Rc<RefCell<Vec<bool>>> =
            Rc::new(RefCell::new(vec![false; repo_groups]));
        let group_preview =
            reorder::preview(&self.reorder, &Surface::SidebarGroups, repo_groups, pointer);
        let repo_roots: Vec<PathBuf> = repo_slots
            .iter()
            .filter_map(|&ix| sections[ix].key.clone())
            .collect();
        let slot_display: Vec<usize> = match &group_preview {
            Some(p) => {
                if let (Some(from), Some(to)) = (repo_roots.get(p.from), repo_roots.get(p.target))
                    && let Some(order) = regrouped_order(&keys, from, to)
                {
                    reorder::set_pending(&self.reorder, &Surface::SidebarGroups, order);
                }
                p.order.clone()
            }
            None => (0..repo_groups).collect(),
        };

        let scratch_slots: Vec<usize> = (0..sections.len())
            .filter(|&ix| sections[ix].key.is_none())
            .filter(rendered)
            .collect();
        let unpinned_headed_count = repo_groups
            + scratch_slots
                .iter()
                .filter(|&&ix| sections[ix].name.is_some())
                .count();
        let show_pin_divider = pinned_groups > 0 && unpinned_headed_count > 0;
        let mut divider_inserted = false;

        let mut blocks: Vec<(Option<(Surface, usize)>, usize)> = pinned_slot_display
            .into_iter()
            .map(|slot| (Some((Surface::PinnedGroups, slot)), pinned_slots[slot]))
            .collect();
        blocks.extend(
            slot_display
                .into_iter()
                .map(|slot| (Some((Surface::SidebarGroups, slot)), repo_slots[slot])),
        );
        blocks.extend(scratch_slots.into_iter().map(|ix| (None, ix)));

        // `scroll_to_item` indexes direct children of this list. A group block
        // would scroll the whole group, so headers, dividers, and rows are
        // counted as their own children except while a group is being dragged.
        let mut painted: usize = 0;
        let mut painted_rows: Vec<(tty7_core::core::machine::TabId, usize)> = Vec::new();

        for (group_slot, group_ix) in blocks {
            let section = &sections[group_ix];
            let content_indent = if section.name.is_some() {
                row_metrics::GROUP_INDENT
            } else {
                0.
            };
            let (drag_slots, bounds_seen, active_preview, anim_prefix) = match &group_slot {
                Some((Surface::PinnedGroups, _)) => (
                    &pinned_group_slots,
                    &pinned_bounds_seen,
                    &pinned_preview,
                    "pinned-group-slide",
                ),
                _ => (
                    &group_slots,
                    &group_bounds_seen,
                    &group_preview,
                    "group-slide",
                ),
            };
            let slot_ix = group_slot.as_ref().map(|(_, slot)| *slot);
            if show_pin_divider && !section.pinned && !divider_inserted {
                divider_inserted = true;
                painted += 1;
                list = list.child(
                    div()
                        .mx(px(12.))
                        .my(px(6.))
                        .h(px(1.))
                        .bg(cx.theme().sidebar_border),
                );
            }
            let group_key = section.key.clone();
            let group_key_str = group_name_key(&group_key);
            let is_ssh_group = group_key
                .as_deref()
                .and_then(ssh_hop_group_target)
                .is_some();
            let stored_collapsed = section.name.is_some()
                && cx
                    .global::<Config>()
                    .sidebar_collapsed_groups
                    .contains(&group_key_str);
            let effective_collapsed = stored_collapsed && query.is_empty();
            let mut rows: Vec<ContextMenu<Stateful<Div>>> = Vec::new();
            let visible = visible_by_section[group_ix].clone();
            let visible_count = visible.len();
            let draw_visible: Vec<usize> = if effective_collapsed {
                Vec::new()
            } else {
                visible.clone()
            };
            let visible_tabs: Vec<usize> = draw_visible.clone();
            let row_slots: Rc<RefCell<Vec<Bounds<Pixels>>>> =
                Rc::new(RefCell::new(vec![Bounds::default(); draw_visible.len()]));
            let row_preview = reorder::preview(
                &self.reorder,
                &Surface::SidebarRows(group_key.clone()),
                draw_visible.len(),
                pointer,
            );
            for (slot, i) in draw_visible.into_iter().enumerate() {
                let badge_pos = badge_pos[i];
                let tab = &self.tabs[i];
                let is_active = i == active;
                let agent_badge = tab.focused_agent_badge(Some(window), cx);
                let agent = agent_badge.agent;
                let avatar = TabAvatar::choose(agent, tab.foreground_app(Some(window), cx));
                let title = tab.title_details(Some(window), cx);
                let ssh = title.ssh_target.is_some();
                let agent_indicator = agent_badge.indicator();
                let git_cwd = diff_click_cwd(
                    cx.global::<Config>(),
                    tab.pane.focused_or_first(window, cx).and_then(|leaf| {
                        let view = leaf.read(cx);
                        let cwd = view.git_status_cwd()?.to_path_buf();
                        Some((view.git_host_id(), cwd))
                    }),
                );
                let badge_extra = if show_badges && badge_pos < 9 {
                    row_metrics::BADGE + row_metrics::GAP
                } else {
                    0.
                };
                // Elision is measured against this budget so the label and
                // branch never wrap or overflow into CSS truncation.
                let label_avail =
                    (row_metrics::text_budget(width) - content_indent - badge_extra).max(48.);
                let title_size = row_metrics::TITLE_REM * rem;
                let meta_size = 0.75 * rem;
                let title_font = if is_active { &title_font_active } else { &font };
                // An agent row opens its second line with a status tag, in the
                // badge's colour, when enabled by configuration. The tag never
                // elides; whatever follows it gives up the room.
                let show_status_text = cx.global::<Config>().sidebar_agent_status_text;
                let status_tag = if show_status_text {
                    agent_indicator.and_then(|s| {
                        s.sidebar_tag()
                            .map(|tag| (tag, s.color(cx.theme().sidebar)))
                    })
                } else {
                    None
                };
                let status_size = row_metrics::STATUS_REM * rem;
                let status_w = status_tag.map_or(0., |(tag, _)| {
                    measure_text(&window.text_system(), &status_font, status_size, tag)
                        + 2. * row_metrics::STATUS_PAD
                        + 0.375 * rem // The following gap_1p5 scales with the UI font size.
                });
                let status_lead = || {
                    status_tag.map(|(tag, colour)| {
                        let mut ink = gpui::Hsla::from(colour);
                        // Latte's yellow and green need deeper ink on a light fill.
                        if !dark_sidebar {
                            ink.l *= 0.5;
                        }
                        div()
                            .id(("sidebar-status", i))
                            .flex_shrink_0()
                            .text_size(px(status_size))
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_height(gpui::relative(1.2))
                            .px(px(row_metrics::STATUS_PAD))
                            .py(px(1.))
                            .rounded(px(4.))
                            .text_color(ink)
                            .bg(colour.opacity(0.24))
                            .child(tag)
                    })
                };
                // Title: elide the *full* label against the row budget, so a
                // wide sidebar shows the whole thing and a narrow one keeps
                // whichever end identifies it — the tail for a path, both
                // edges for anything else. A fixed segment cap
                // (`short_title`) would elide even when the row has room, so
                // only the width may decide here.
                //
                // `full_title` is the unelided string the card can expand
                // back to; `None` means the row is showing a placeholder that
                // no card can improve on.
                let (shown_title, full_title) = if title.label.trim().is_empty() {
                    (
                        SharedString::from(t_fmt(
                            L10nKey::TabUnnamedShell,
                            &[("n", &((i + 1).to_string()))],
                        )),
                        None,
                    )
                } else {
                    let full = SharedString::from(title.label.clone());
                    let shown = if ssh {
                        elide_keep_edges(
                            &window.text_system(),
                            title_font,
                            title_size,
                            &full,
                            label_avail,
                        )
                    } else {
                        elide_label(
                            &window.text_system(),
                            title_font,
                            title_size,
                            &full,
                            label_avail,
                        )
                    };
                    (shown, Some(full))
                };
                let git_context = title.context.as_ref().map(|context| {
                    let shown = elide_keep_edges(
                        &window.text_system(),
                        &font,
                        meta_size,
                        context,
                        (label_avail - status_w).max(0.) * 0.6,
                    );
                    (shown, SharedString::from(context.clone()))
                });
                let git_context_w = git_context.as_ref().map_or(0., |(context, _)| {
                    measure_text(&window.text_system(), &font, meta_size, context)
                        + row_metrics::META_GAP
                });
                let mut branch_shown: Option<(SharedString, SharedString, u32, u32)> = None;
                let mut cwd_shown = git_context.clone();
                let git_line = tab.git_status(Some(window), cx).map(|g| {
                    let mut line = h_flex()
                        .id(("sidebar-git", i))
                        .w_full()
                        .items_center()
                        .gap_1p5()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .children(status_lead())
                        .children(
                            git_context
                                .as_ref()
                                .map(|(context, _)| div().flex_shrink_0().child(context.clone())),
                        )
                        .child(
                            gpui::svg()
                                .path("icons/git-branch.svg")
                                .flex_shrink_0()
                                .size(px(row_metrics::BRANCH_ICON))
                                .text_color(cx.theme().muted_foreground),
                        );
                    // The diff counts are measured against real glyphs so the
                    // branch can be elided to exactly the space they leave;
                    // the counts themselves never wrap or shrink. They render
                    // as two children of a `gap_1p5` row, so the gap between
                    // them is measured rather than a space that stands in for
                    // it.
                    let mut counts_w = 0.;
                    if g.added > 0 {
                        counts_w += measure_text(
                            &window.text_system(),
                            &font,
                            meta_size,
                            &format!("+{}", g.added),
                        );
                    }
                    if g.removed > 0 {
                        counts_w += measure_text(
                            &window.text_system(),
                            &font,
                            meta_size,
                            &format!("−{}", g.removed),
                        );
                    }
                    if g.added > 0 && g.removed > 0 {
                        counts_w += row_metrics::META_GAP;
                    }
                    if counts_w > 0. {
                        // The gap between the branch and the counts.
                        counts_w += row_metrics::META_GAP;
                    }
                    // Branch: keep both ends (`window-…backdrop`) so its
                    // identifying tail survives a narrow sidebar.
                    let branch_avail = (label_avail
                        - status_w
                        - git_context_w
                        - row_metrics::BRANCH_ICON
                        - row_metrics::META_GAP
                        - counts_w)
                        .max(0.);
                    let shown = elide_keep_edges(
                        &window.text_system(),
                        &font,
                        meta_size,
                        &g.branch,
                        branch_avail,
                    );
                    branch_shown = Some((
                        shown.clone(),
                        SharedString::from(g.branch.clone()),
                        g.added,
                        g.removed,
                    ));
                    line = line.child(div().flex_1().min_w_0().truncate().child(shown));
                    if g.added > 0 || g.removed > 0 {
                        let mut counts = h_flex()
                            .id(("sidebar-diff", i))
                            .debug_selector(move || format!("sidebar-diff-{i}"))
                            .flex_shrink_0()
                            .items_center()
                            .gap_1p5()
                            .when_some(git_cwd, |counts, (host, cwd)| {
                                // A click target inside a click target: the row
                                // highlights as a whole, which says nothing
                                // about the counts being their own button. The
                                // underline the SFTP breadcrumb uses for
                                // clickable text says where this one starts.
                                counts
                                    .cursor_pointer()
                                    .hover(|s| s.underline())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                                            cx.stop_propagation();
                                            // Swallowing the press also swallows the
                                            // row's click, the only thing that
                                            // activates a tab — so this row has to
                                            // activate itself, or the overlay lands
                                            // in whichever tab was already on
                                            // screen, carrying this row's repo (#706).
                                            this.activate(i, window, cx);
                                            this.toggle_diff_overlay(host, cwd.clone(), window, cx);
                                        }),
                                    )
                            });
                        if g.added > 0 {
                            counts = counts.child(
                                div()
                                    .text_color(cx.theme().success)
                                    .child(format!("+{}", g.added)),
                            );
                        }
                        if g.removed > 0 {
                            counts = counts.child(
                                div()
                                    .text_color(cx.theme().danger)
                                    .child(format!("−{}", g.removed)),
                            );
                        }
                        line = line.child(counts);
                    }
                    line
                });
                // Outside a repo there is no branch line; the second line then
                // carries the compressed cwd with its root marker, so a tab
                // whose title is just a shell name still says where it lives.
                if git_line.is_none() {
                    cwd_shown = if ssh {
                        title.context.as_ref().map(|context| {
                            let full = SharedString::from(context.clone());
                            let shown = elide_keep_edges(
                                &window.text_system(),
                                &font,
                                meta_size,
                                &full,
                                (label_avail - status_w).max(0.),
                            );
                            (shown, full)
                        })
                    } else {
                        tab.pane
                            .focused_or_first(window, cx)
                            .and_then(|leaf| {
                                let leaf = leaf.read(cx);
                                Some((leaf.effective_cwd()?, leaf.display_home(cx)))
                            })
                            .map(|(cwd, home)| {
                                let text = cwd.display().to_string();
                                let full = SharedString::from(
                                    abbreviate_home(&text, home.as_deref()).into_owned(),
                                );
                                let shown = elide_path_keep_tail(
                                    &window.text_system(),
                                    &font,
                                    meta_size,
                                    &full,
                                    (label_avail - status_w).max(0.),
                                );
                                (shown, full)
                            })
                            // The title already carries the whole path; a second
                            // copy adds noise, not information.
                            .filter(|(_, full)| {
                                full_title
                                    .as_ref()
                                    .map_or(shown_title.as_ref(), |t| t.as_ref())
                                    != full.as_ref()
                            })
                    };
                }
                // Outside a repo a known status still gets a line, even when
                // the cwd was dropped as a copy of the title.
                let cwd_line = git_line
                    .is_none()
                    .then(|| {
                        let cwd = cwd_shown.as_ref().map(|(cwd, _)| cwd.clone());
                        (status_tag.is_some() || cwd.is_some()).then(|| {
                            h_flex()
                                .id(("sidebar-cwd", i))
                                .w_full()
                                .items_center()
                                .gap_1p5()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground.opacity(0.8))
                                .children(status_lead())
                                .children(
                                    cwd.map(|cwd| div().flex_1().min_w_0().truncate().child(cwd)),
                                )
                        })
                    })
                    .flatten();
                let rename_input = self
                    .renaming
                    .as_ref()
                    .filter(|r| r.tab == tab.tree_id.get())
                    .map(|r| r.input.clone());

                let shown = SidebarRowShown {
                    title: full_title.map(|full| (shown_title.clone(), full)),
                    branch: branch_shown.clone(),
                    cwd: cwd_shown.clone(),
                };
                let info = self.sidebar_info(tab, window, cx, &shown);
                // Colors are captured by value so the tooltip builder (which
                // borrows no app state) can style the card on its own.
                let muted = cx.theme().muted_foreground;
                let success = cx.theme().success;
                let danger = cx.theme().danger;

                let label_region = match rename_input {
                    Some(input) => div()
                        .id(("sidebar-rename", i))
                        .flex_1()
                        .min_w_0()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        // The row switches tabs on the *release* now, so
                        // holding the press back is no longer enough: a click
                        // landing in the field would reach the row behind it
                        // and switch away from the name being typed, taking
                        // the focus with it.
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(Input::new(&input).appearance(false))
                        .into_any_element(),
                    None => v_flex()
                        .id(("sidebar-label", i))
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.))
                        .when_some(info, |col, info| {
                            col.tooltip(move |window, cx| {
                                // `Tooltip::element` rebuilds its content on
                                // every hover, so the captured info is cloned
                                // per call instead of being moved out.
                                let info = info.clone();
                                gpui_component::tooltip::Tooltip::element(move |_window, _cx| {
                                    let card = v_flex()
                                        .gap_1()
                                        // The card is the one place that
                                        // promised the whole string, so a long
                                        // path wraps here rather than being
                                        // truncated a second time.
                                        .when_some(info.title.clone(), |c, title| {
                                            c.child(
                                                div()
                                                    .max_w(px(420.))
                                                    .text_size(px(title_size))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(title),
                                            )
                                        })
                                        .when_some(
                                            info.branch.clone(),
                                            |c, (branch, added, removed)| {
                                                let mut line = h_flex()
                                                    .items_center()
                                                    .gap_1p5()
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .child(
                                                        gpui::svg()
                                                            .path("icons/git-branch.svg")
                                                            .flex_shrink_0()
                                                            .size(px(11.))
                                                            .text_color(muted),
                                                    )
                                                    .child(div().child(branch));
                                                if added > 0 {
                                                    line = line.child(
                                                        div()
                                                            .text_color(success)
                                                            .child(format!("+{added}")),
                                                    );
                                                }
                                                if removed > 0 {
                                                    line = line.child(
                                                        div()
                                                            .text_color(danger)
                                                            .child(format!("−{removed}")),
                                                    );
                                                }
                                                c.child(line)
                                            },
                                        )
                                        .when_some(info.cwd.clone(), |c, cwd| {
                                            c.child(
                                                div()
                                                    .max_w(px(420.))
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .child(cwd),
                                            )
                                        })
                                        .when_some(info.host.clone(), |c, host| {
                                            c.child(
                                                h_flex()
                                                    .items_center()
                                                    .gap_1p5()
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .child(
                                                        gpui::svg()
                                                            .path("icons/machine-remote.svg")
                                                            .flex_shrink_0()
                                                            .size(px(11.))
                                                            .text_color(muted),
                                                    )
                                                    .child(div().truncate().child(host)),
                                            )
                                        });
                                    card
                                })
                                .build(window, cx)
                            })
                        })
                        .child(
                            div()
                                .w_full()
                                .truncate()
                                .text_size(px(title_size))
                                .when(is_active, |d| d.font_weight(FontWeight::MEDIUM))
                                .child(shown_title),
                        )
                        .children(git_line)
                        .children(cwd_line)
                        .into_any_element(),
                };

                let row = h_flex()
                    .id(("tab-row", i))
                    .debug_selector(move || format!("sidebar-tab-row-{i}"))
                    .group(SharedString::from(format!("tab-row-{i}")))
                    .cursor_pointer()
                    .on_drag(DragTab, {
                        let state = self.reorder.clone();
                        let slots = row_slots.clone();
                        let group_key = group_key.clone();
                        let id = tab.tree_id.get();
                        move |_drag, grab, _window, cx| {
                            cx.stop_propagation();
                            *state.borrow_mut() = Some(
                                Reorder::new(
                                    Surface::SidebarRows(group_key.clone()),
                                    slot,
                                    slots.borrow().clone(),
                                    Axis::Vertical,
                                    px(ROW_GAP),
                                    grab,
                                )
                                .of_tab(id),
                            );
                            cx.new(|_| DragTab)
                        }
                    })
                    .w_full()
                    .py_1()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .pl_2()
                    .pr_2()
                    .rounded_lg()
                    .when(is_active, |s| {
                        s.bg(cx.theme().sidebar_accent)
                            .text_color(cx.theme().sidebar_accent_foreground)
                    })
                    .when(!is_active, |s| {
                        s.text_color(cx.theme().sidebar_foreground)
                            .hover(|s| s.bg(gpui::rgb(sf.hover)))
                    })
                    .when(row_preview.as_ref().is_some_and(|p| p.from == slot), |s| {
                        s.opacity(0.75)
                    })
                    .child(
                        canvas(
                            {
                                let slots = row_slots.clone();
                                // The row by tab as well as by slot: reordering
                                // reads the slots of one group, a pane dropped
                                // on the sidebar reads every row there is.
                                let by_tab = self.sidebar_slots.clone();
                                move |bounds, _window, _cx| {
                                    if let Some(s) = slots.borrow_mut().get_mut(slot) {
                                        *s = bounds;
                                    }
                                    if let Some(s) = by_tab.borrow_mut().get_mut(i) {
                                        *s = bounds;
                                    }
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                    // Switched on the release, not the press: a press that turns
                    // into a drag is the tab being picked up, and a tab on its
                    // way into another tab's layout must not put itself on
                    // screen on the way there.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.activate(i, window, cx);
                    }))
                    .child(
                        div()
                            .debug_selector(move || format!("sidebar-tab-avatar-{i}"))
                            .flex_shrink_0()
                            .ml(px(content_indent))
                            .child(self.tab_avatar(
                                ("sidebar-avatar", i),
                                avatar,
                                agent_indicator,
                                22.,
                                ssh,
                                cx,
                            )),
                    )
                    .child(label_region)
                    .when(show_badges && badge_pos < 9, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(20.))
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if is_active {
                                    cx.theme().sidebar_accent_foreground
                                } else {
                                    cx.theme().muted_foreground
                                })
                                .child(tab_badge_label(badge_pos)),
                        )
                    })
                    .when(!(show_badges && badge_pos < 9), |row| {
                        let backing: gpui::Hsla = if is_active {
                            gpui::rgb(sf.selected).into()
                        } else {
                            gpui::rgb(sf.hover).into()
                        };
                        let mut fade_from = backing;
                        fade_from.a = 0.;
                        row.child(
                            h_flex()
                                .absolute()
                                .top(px(4.))
                                .right(px(6.))
                                .opacity(0.)
                                .group_hover(SharedString::from(format!("tab-row-{i}")), |s| {
                                    s.opacity(1.)
                                })
                                .child(div().w(px(10.)).h(px(crate::ui::tab_strip::MIN_TARGET)).bg(
                                    linear_gradient(
                                        90.,
                                        linear_color_stop(fade_from, 0.),
                                        linear_color_stop(backing, 1.),
                                    ),
                                ))
                                .child(
                                    div().bg(backing).child(
                                        crate::ui::tab_strip::hit_target(
                                            Button::new(("sidebar-close", i))
                                                .icon(IconName::Close)
                                                .ghost()
                                                .xsmall(),
                                        )
                                        .tooltip(t(L10nKey::TabContextCloseTab))
                                        // Held here, because the row behind it
                                        // switches tabs on the release too:
                                        // without this the same click closes
                                        // tab `i` and then activates whichever
                                        // tab slid into its place.
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.close_tab(i, window, cx);
                                            }),
                                        ),
                                    ),
                                ),
                        )
                    });

                let menu_app = cx.entity().downgrade();
                rows.push(row.context_menu(move |menu, window, cx| {
                    Tty7App::tab_context_menu(menu, i, true, &menu_app, window, cx)
                }));
            }

            let kept_empty_pin = section.pinned && section.tabs.is_empty() && query.is_empty();
            if visible_count == 0 && !kept_empty_pin {
                continue;
            }

            let row_display: Vec<usize> = match &row_preview {
                Some(p) => {
                    if let Some(order) = reordered_rows(
                        &keys,
                        &pinned_cfg,
                        &keep_empty,
                        &group_key,
                        &visible_tabs,
                        p.from,
                        p.target,
                    ) {
                        reorder::set_pending(
                            &self.reorder,
                            &Surface::SidebarRows(group_key.clone()),
                            order,
                        );
                    }
                    p.order.clone()
                }
                None => (0..rows.len()).collect(),
            };
            let row_ids: Vec<tty7_core::core::machine::TabId> = row_display
                .iter()
                .map(|&slot| self.tabs[visible_tabs[slot]].tree_id.get())
                .collect();
            let tab_count = section.tabs.len();
            let mut rows: Vec<Option<ContextMenu<Stateful<Div>>>> =
                rows.into_iter().map(Some).collect();
            let mut rows: Vec<AnyElement> = row_display
                .into_iter()
                .map(|slot| match &row_preview {
                    Some(p) if p.from == slot => deferred(
                        rows[slot]
                            .take()
                            .expect("each slot emitted once")
                            .relative()
                            .top(p.held),
                    )
                    .into_any_element(),
                    Some(p) => {
                        let offset = p.offsets[slot].as_f32();
                        rows[slot]
                            .take()
                            .expect("each slot emitted once")
                            .with_animation(
                                (
                                    SharedString::from(format!("row-slide-{}", p.generation)),
                                    slot,
                                ),
                                Animation::new(std::time::Duration::from_millis(REORDER_SLIDE_MS))
                                    .with_easing(ease_out_quint()),
                                move |el, delta| el.top(px(offset * (1. - delta))),
                            )
                            .into_any_element()
                    }
                    None => rows[slot]
                        .take()
                        .expect("each slot emitted once")
                        .into_any_element(),
                })
                .collect();

            if kept_empty_pin
                && !effective_collapsed
                && let Some(path) = section.key.clone()
            {
                let shown_name = section
                    .name
                    .clone()
                    .map(|n| effective_group_name(cx.global::<Config>(), &section.key, n))
                    .unwrap_or_default();
                let placeholder_label = t_fmt(L10nKey::SidebarNewTabIn, &[("name", &shown_name)]);
                let ghost = h_flex()
                    .id(("sidebar-ghost", group_ix))
                    .group(SharedString::from(format!("sidebar-ghost-{group_ix}")))
                    .cursor_pointer()
                    .w_full()
                    .py_1()
                    .pl_2()
                    .pr_2()
                    .gap_2()
                    .items_center()
                    .rounded_lg()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .hover(|s| {
                        s.bg(gpui::rgb(sf.hover))
                            .text_color(cx.theme().sidebar_foreground)
                    })
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.new_tab_at(path.clone(), window, cx);
                    }))
                    .child(
                        div()
                            .flex_shrink_0()
                            .ml(px(content_indent))
                            .size(px(row_metrics::AVATAR))
                            .rounded_full()
                            .border_1()
                            .border_dashed()
                            .border_color(cx.theme().muted_foreground.opacity(0.6))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::empty()
                                    .path("icons/plus.svg")
                                    .size(px(11.))
                                    .text_color(cx.theme().muted_foreground),
                            ),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(placeholder_label))
                    .into_any_element();
                rows.push(ghost);
            }

            let header = section.name.clone().map(|name| {
                let shown = effective_group_name(cx.global::<Config>(), &section.key, name);
                let rename_input = self
                    .group_rename
                    .as_ref()
                    .filter(|r| r.key == section.key)
                    .map(|r| r.input.clone());
                let has_active = section.tabs.contains(&active);
                let folded_active = effective_collapsed && has_active;
                let header_group = SharedString::from(format!("sidebar-group-head-{group_ix}"));
                let fg = cx.theme().sidebar_foreground;
                let muted = cx.theme().muted_foreground;
                let accent = cx.theme().primary;
                let hover_bg = gpui::rgb(sf.hover);
                let base = h_flex()
                    .id(("sidebar-group", group_ix))
                    .group(header_group.clone())
                    .relative()
                    .w_full()
                    .h(px(28.))
                    .items_center()
                    .gap_1()
                    .pl(px(LEADING_ICON_CENTER - row_metrics::LIST_PAD - 8.))
                    .pr_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .text_size(px(13.))
                    .text_color(if folded_active { fg } else { muted })
                    .hover(move |s| s.bg(hover_bg).text_color(fg))
                    .when_some(group_slot.clone(), |header, (surface, slot)| {
                        let slots = drag_slots.clone();
                        crate::ui::reorder::cursor_grab(header).on_drag(DragGroup, {
                            let state = self.reorder.clone();
                            move |_drag, grab, _window, cx| {
                                cx.stop_propagation();
                                *state.borrow_mut() = Some(Reorder::new(
                                    surface.clone(),
                                    slot,
                                    slots.borrow().clone(),
                                    Axis::Vertical,
                                    px(ROW_GAP),
                                    grab,
                                ));
                                cx.new(|_| DragGroup)
                            }
                        })
                    });
                if let Some(input) = rename_input {
                    return base
                        .child(
                            div()
                                .id(("sidebar-group-rename", group_ix))
                                .flex_1()
                                .min_w_0()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                // Same guard as the tab rows' rename box: a
                                // click landing in the field must not reach
                                // the drag handler behind it and take the
                                // focus with it.
                                .on_click(|_, _, cx| cx.stop_propagation())
                                .child(Input::new(&input).appearance(false)),
                        )
                        .into_any_element();
                }

                let chevron_path = if effective_collapsed {
                    "icons/chevron-right-thin.svg"
                } else {
                    "icons/chevron-down-thin.svg"
                };
                let chevron_color = if folded_active { accent } else { muted };
                let cfg = cx.global::<Config>();
                let show_arrow = cfg.sidebar_group_arrows;
                let folder_path = group_folder_icon(
                    section.key.is_some(),
                    cfg.sidebar_group_folders,
                    show_arrow,
                    effective_collapsed,
                );
                let show_chevron = header_shows_chevron(
                    section.key.is_some(),
                    show_arrow,
                    cfg.sidebar_group_folders,
                );
                let chevron = div()
                    .debug_selector(move || format!("sidebar-group-chevron-{group_ix}"))
                    .flex_shrink_0()
                    .size(px(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(chevron_color)
                    .when(!folded_active, |d| {
                        d.group_hover(header_group.clone(), move |s| s.text_color(fg))
                    })
                    .child(Icon::empty().path(chevron_path).size(px(12.)));
                let folder = folder_path.map(|path| {
                    div()
                        .debug_selector(move || format!("sidebar-group-folder-{group_ix}"))
                        .flex_shrink_0()
                        .size(px(if show_arrow { 13. } else { 16. }))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(chevron_color)
                        .when(!folded_active, |d| {
                            d.group_hover(header_group.clone(), move |s| s.text_color(fg))
                        })
                        .child(Icon::empty().path(path).size(px(13.)))
                });

                let status_dots: Option<Div> = if effective_collapsed {
                    let dot_colors: Vec<gpui::Rgba> = section
                        .tabs
                        .iter()
                        .filter_map(|&ti| {
                            self.tabs[ti]
                                .focused_agent_badge(Some(window), cx)
                                .indicator()
                        })
                        .filter(|&ind| ind != crate::ui::status_indicator::StatusIndicator::Unknown)
                        .map(|ind| ind.color(cx.theme().sidebar))
                        .collect();
                    (!dot_colors.is_empty()).then(|| {
                        let extra = dot_colors.len().saturating_sub(4);
                        let mut row = h_flex().flex_shrink_0().items_center().gap(px(3.)).ml_1();
                        for color in dot_colors.into_iter().take(4) {
                            row = row.child(div().size(px(6.)).rounded_full().bg(color));
                        }
                        if extra > 0 {
                            row = row.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .child(format!("+{extra}")),
                            );
                        }
                        row
                    })
                } else {
                    None
                };

                let label: SharedString = shown.clone().into();
                let menu_app = cx.entity().downgrade();
                let dropdown_app = menu_app.clone();
                let key = section.key.clone();
                let dropdown_key = key.clone();
                let click_key_str = group_key_str.clone();
                let click_headed = headed_keys.clone();
                let menu_headed = headed_keys.clone();
                let dropdown_headed = headed_keys.clone();
                let is_pinned = section.pinned;
                let current_for_menu = shown.clone();
                let current_for_dropdown = shown.clone();

                let actions = h_flex()
                    .id(("sidebar-group-actions", group_ix))
                    .absolute()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .pl_1p5()
                    .pr_1p5()
                    .rounded_md()
                    .bg(hover_bg)
                    .items_center()
                    .gap(px(2.))
                    .invisible()
                    .group_hover(header_group.clone(), |s| s.visible())
                    .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .when(is_local && section.key.is_some() && !is_ssh_group, |act| {
                        let pin_key = group_key_str.clone();
                        let icon_path = if is_pinned {
                            "icons/pin-off.svg"
                        } else {
                            "icons/pin.svg"
                        };
                        let tip = if is_pinned {
                            t(L10nKey::SidebarUnpinGroup)
                        } else {
                            t(L10nKey::SidebarPinGroup)
                        };
                        act.child(
                            Button::new(("sidebar-group-pin", group_ix))
                                .icon(Icon::empty().path(icon_path))
                                .ghost()
                                .xsmall()
                                .tooltip(tip)
                                .on_click(cx.listener(move |this, _, _window, cx| {
                                    cx.stop_propagation();
                                    this.toggle_pin_group(&pin_key, cx);
                                })),
                        )
                    })
                    .when(!is_ssh_group, |act| {
                        act.child({
                            let new_key = section.key.clone();
                            Button::new(("sidebar-group-new", group_ix))
                                .icon(Icon::empty().path("icons/plus.svg"))
                                .ghost()
                                .xsmall()
                                .tooltip(t(L10nKey::SidebarNewTabHere))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.new_tab_in_group(new_key.clone(), window, cx);
                                }))
                        })
                    })
                    .child(
                        Button::new(("sidebar-group-more", group_ix))
                            .icon(Icon::empty().path("icons/ellipsis.svg"))
                            .ghost()
                            .xsmall()
                            .tooltip(t(L10nKey::TabTooltipMore))
                            .dropdown_menu(move |menu, _window, _cx| {
                                Self::group_header_menu(
                                    menu,
                                    &dropdown_app,
                                    dropdown_key.clone(),
                                    current_for_dropdown.clone(),
                                    is_pinned,
                                    is_local,
                                    dropdown_headed.clone(),
                                )
                            }),
                    );

                base.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, ev: &ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        if ev.modifiers().alt {
                            this.collapse_other_groups(&click_key_str, &click_headed, cx);
                        } else {
                            this.toggle_group_collapsed(&click_key_str, cx);
                        }
                    }))
                    .when(show_chevron, |header| header.child(chevron))
                    .children(folder)
                    .when(section.key.is_none(), |header| {
                        header.child(
                            div()
                                .flex_shrink_0()
                                .size(px(if show_chevron { 13. } else { 16. }))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(chevron_color)
                                .when(!folded_active, |d| {
                                    d.group_hover(header_group.clone(), move |s| s.text_color(fg))
                                })
                                .child(Icon::new(IconName::Inbox).size(px(13.))),
                        )
                    })
                    .when(is_ssh_group, |b| {
                        // A quiet marker: a muted outline pill with no fill
                        // and no colour of its own. The row badge already says
                        // SSH in blue; the header only needs to read as one.
                        b.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(10.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .line_height(gpui::relative(1.2))
                                .px(px(5.))
                                .py(px(1.))
                                .rounded_full()
                                .border_1()
                                .border_color(muted.opacity(0.45))
                                .text_color(muted)
                                .child("SSH"),
                        )
                    })
                    .child(
                        div()
                            .flex_shrink(1.)
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .when(
                                folded_active && !show_chevron && folder_path.is_none(),
                                |label| label.text_color(accent),
                            )
                            .child(label),
                    )
                    .when(is_pinned, |b| {
                        b.child(
                            div()
                                .flex_shrink_0()
                                .size(px(12.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(muted)
                                .child(Icon::empty().path("icons/pin-filled.svg").size(px(11.))),
                        )
                    })
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(muted.opacity(0.7))
                            .child(tab_count.to_string()),
                    )
                    .children(status_dots)
                    .child(actions)
                    .context_menu(move |menu, _window, _cx| {
                        Self::group_header_menu(
                            menu,
                            &menu_app,
                            key.clone(),
                            current_for_menu.clone(),
                            is_pinned,
                            is_local,
                            menu_headed.clone(),
                        )
                    })
                    .into_any_element()
            });

            // While groups are sliding, keep one block so the drag measures a
            // single rectangle. Otherwise each header and row is its own list
            // child, which is what `scroll_to_item` can land on.
            let dragging_group = active_preview.is_some() && slot_ix.is_some();
            any_rows = true;
            if dragging_group {
                let block_index = painted;
                painted += 1;
                for id in &row_ids {
                    painted_rows.push((*id, block_index));
                }
                let block = v_flex()
                    .w_full()
                    .gap(px(ROW_GAP))
                    .when(
                        active_preview
                            .as_ref()
                            .is_some_and(|p| Some(p.from) == slot_ix),
                        |b| b.opacity(0.75),
                    )
                    .children(header)
                    .children(rows)
                    .when_some(group_slot.clone(), |block, (_, slot)| {
                        let slots = drag_slots.clone();
                        block.child(
                            canvas(
                                move |bounds, _window, _cx| {
                                    if let Some(s) = slots.borrow_mut().get_mut(slot) {
                                        *s = bounds;
                                    }
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .inset_0(),
                        )
                    });
                list = list.child(match (active_preview, slot_ix) {
                    (Some(p), Some(slot)) if p.from == slot => {
                        deferred(block.relative().top(p.held)).into_any_element()
                    }
                    (Some(p), Some(slot)) => {
                        let offset = p.offsets[slot].as_f32();
                        block
                            .with_animation(
                                (
                                    SharedString::from(format!("{anim_prefix}-{}", p.generation)),
                                    slot,
                                ),
                                Animation::new(std::time::Duration::from_millis(REORDER_SLIDE_MS))
                                    .with_easing(ease_out_quint()),
                                move |el, delta| el.top(px(offset * (1. - delta))),
                            )
                            .into_any_element()
                    }
                    _ => block.into_any_element(),
                });
            } else {
                let mut pieces: Vec<(Option<tty7_core::core::machine::TabId>, AnyElement)> =
                    Vec::new();
                if let Some(header) = header {
                    pieces.push((None, header));
                }
                let mut ids = row_ids.into_iter().map(Some).collect::<Vec<_>>();
                while ids.len() < rows.len() {
                    ids.push(None);
                }
                pieces.extend(ids.into_iter().zip(rows));
                for (tab_id, el) in pieces {
                    if let Some(id) = tab_id {
                        painted_rows.push((id, painted));
                    }
                    painted += 1;
                    let slots = drag_slots.clone();
                    let seen = bounds_seen.clone();
                    let measured = slot_ix.map(|slot| {
                        canvas(
                            move |bounds, _window, _cx| {
                                if bounds.size.width.as_f32() == 0.
                                    && bounds.size.height.as_f32() == 0.
                                {
                                    return;
                                }
                                let Ok(mut slots) = slots.try_borrow_mut() else {
                                    return;
                                };
                                let Ok(mut seen) = seen.try_borrow_mut() else {
                                    return;
                                };
                                let (Some(slot_bounds), Some(already)) =
                                    (slots.get_mut(slot), seen.get_mut(slot))
                                else {
                                    return;
                                };
                                if !*already {
                                    *slot_bounds = bounds;
                                    *already = true;
                                } else {
                                    let x = slot_bounds.origin.x.min(bounds.origin.x);
                                    let y = slot_bounds.origin.y.min(bounds.origin.y);
                                    let right = (slot_bounds.origin.x + slot_bounds.size.width)
                                        .max(bounds.origin.x + bounds.size.width);
                                    let bottom = (slot_bounds.origin.y + slot_bounds.size.height)
                                        .max(bounds.origin.y + bounds.size.height);
                                    *slot_bounds = Bounds {
                                        origin: point(x, y),
                                        size: size(right - x, bottom - y),
                                    };
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0()
                    });
                    list = list.child(
                        div()
                            .w_full()
                            .relative()
                            .child(el)
                            .children(measured)
                            .into_any_element(),
                    );
                }
            }
        }

        self.drive_path_reveal_scroll(&painted_rows);

        if !any_rows && !query.is_empty() {
            list = list.child(
                div()
                    .px_2()
                    .py_3()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(crate::ui::i18n::t_fmt(
                        crate::ui::i18n::L10nKey::SettingsNothingMatches,
                        &[("query", &query)],
                    )),
            );
        }

        let controls = h_flex()
            .flex_shrink_0()
            .h(px(TITLE_BAR_HEIGHT))
            .border_b_1()
            .border_color(cx.theme().transparent)
            .items_center()
            .justify_end()
            .gap(px(2.))
            .pr(px(crate::ui::app::tile_trailing_inset()))
            .when_some(crate::ui::app::window_mark(), |row, mark| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .pl(px(crate::ui::app::CONTENT_INSET))
                        .child(mark),
                )
                .child(div().flex_1().min_w(px(GRAB_HANDLE_W)))
            })
            .child(
                div()
                    .occlude()
                    .flex_shrink_0()
                    .child(self.new_tab_button("sidebar-add", cx)),
            )
            .child(
                div().occlude().flex_shrink_0().child(
                    crate::ui::tab_strip::chrome_tile(
                        Button::new("sidebar-collapse")
                            .icon(Icon::empty().path("icons/panel-left.svg")),
                        false,
                        cx,
                    )
                    .rounded_lg()
                    .tooltip(crate::ui::tab_strip::chord_hint(
                        t(L10nKey::TabTooltipHideSidebar),
                        "ToggleLeftPanel",
                        cx,
                    ))
                    .on_click(cx.listener(|this, _, _window, cx| this.toggle_left_panel(cx))),
                ),
            );
        // The tile inside asks for `w_full`, and a percentage is only a width
        // while some box above it has a real one. This row used to have none of
        // its own and borrowed the column's by cross-axis stretch, which did not
        // always hold; `w_full` here swapped that for a second percentage, and a
        // row whose width is `Percent` is no longer `auto`, so it lost stretch
        // as well — on the passes that size the column from its content there
        // was still nothing to resolve against and the tile fell back to hugging
        // the workspace name. Hand the row real pixels: the rail is
        // `w(px(width))` and layout is border-box, so its content is one pixel
        // narrower than that because of the right border.
        let workspace_head = h_flex()
            .w(px(width - 1.))
            .flex_shrink_0()
            .px(px(crate::ui::app::CONTENT_INSET - 7.))
            .pt(px(4.))
            .child(self.workspace_head(cx));

        let chip_inset = LEADING_ICON_CENTER - Self::AVATAR_PX / 2.;
        let toggle_all_btn = (!headed_keys.is_empty()).then(|| {
            let toggle_headed = headed_keys.clone();
            let icon_path = if all_collapsed {
                "icons/chevrons-up-down.svg"
            } else {
                "icons/chevrons-down-up.svg"
            };
            let tip = if all_collapsed {
                t(L10nKey::SidebarExpandAll)
            } else {
                t(L10nKey::SidebarCollapseAll)
            };
            div().flex_shrink_0().child(
                Button::new("sidebar-toggle-all")
                    .icon(Icon::empty().path(icon_path))
                    .ghost()
                    .xsmall()
                    .tooltip(tip)
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        this.set_all_groups_collapsed(&toggle_headed, !all_collapsed, cx);
                    })),
            )
        });
        let top_bar = h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(6.))
            .h(px(44.))
            .pl(px(chip_inset))
            .pr(px(crate::ui::app::CONTENT_INSET))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(Self::AVATAR_PX))
                    .debug_selector(|| "sidebar-search-icon".into())
                    .child(
                        Icon::new(IconName::Search)
                            .size(px(14.))
                            .text_color(cx.theme().muted_foreground),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&self.sidebar_search).appearance(false).pl_0()),
            )
            .children(toggle_all_btn);

        let container: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::new(Cell::new(None));
        // Read while there is still a `cx` to read it from: the drag handler
        // below only ever sees a `Window`, and the cap it clamps against has to
        // be the same one the layout applies or the sidebar springs back from
        // wherever it was dropped.
        let others_floor = self.right_panel_floor(cx) + self.document_floor(cx);
        let backing = canvas(
            {
                let container = container.clone();
                move |bounds, _window, _cx| container.set(Some(bounds))
            },
            {
                let container = container.clone();
                let width_cell = self.sidebar_width.clone();
                let dragging = self.sidebar_dragging.clone();
                move |_bounds, _state, window, _cx| {
                    window.on_mouse_event({
                        let container = container.clone();
                        let width_cell = width_cell.clone();
                        let dragging = dragging.clone();
                        move |ev: &MouseMoveEvent, _phase, window, _cx| {
                            if !dragging.get() {
                                return;
                            }
                            let Some(b) = container.get() else {
                                return;
                            };
                            let raw = (ev.position.x - b.origin.x).as_f32();
                            let max = crate::ui::app::side_panel_max(
                                window.viewport_size().width.as_f32(),
                                MIN_SIDEBAR_WIDTH,
                                others_floor,
                            );
                            width_cell.set(raw.clamp(MIN_SIDEBAR_WIDTH, max));
                            window.refresh();
                        }
                    });
                    window.on_mouse_event({
                        let width_cell = width_cell.clone();
                        let dragging = dragging.clone();
                        move |_ev: &MouseUpEvent, _phase, window, cx| {
                            if !dragging.get() {
                                return;
                            }
                            dragging.set(false);
                            let w = width_cell.get();
                            let cfg = cx.global_mut::<Config>();
                            if cfg.sidebar_width != w {
                                cfg.sidebar_width = w;
                                cfg.save();
                            }
                            window.refresh();
                        }
                    });
                }
            },
        )
        .absolute()
        .size_full();

        let handle_active = self.sidebar_dragging.get();
        let handle = div()
            .group("sidebar-resize")
            .occlude()
            .absolute()
            .top_0()
            .right(px(-(RESIZE_HANDLE_WIDTH / 2.)))
            .w(px(RESIZE_HANDLE_WIDTH))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_col_resize()
            .child(
                div()
                    .w(px(1.))
                    .h_full()
                    .when(handle_active, |d| d.bg(cx.theme().drag_border))
                    .group_hover("sidebar-resize", |s| s.bg(cx.theme().drag_border)),
            )
            .on_mouse_down(MouseButton::Left, {
                let dragging = self.sidebar_dragging.clone();
                move |_ev, window, _cx| {
                    dragging.set(true);
                    window.refresh();
                }
            });

        div()
            .debug_selector(|| "session-sidebar".into())
            .relative()
            .flex_shrink_0()
            .w(px(width))
            .h_full()
            .bg(crate::ui::theme::workspace_surface_color(cx))
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(backing)
            .child(
                // Real pixels, not `size_full`: the rail's own width is a
                // definite `px`, but a percentage off it is still a percentage,
                // and on the passes that size this column from its content it
                // resolves against nothing. Everything below asks for `w_full`
                // — the tab rows, their group blocks, the scroll area — so one
                // unresolved link here collapsed the whole chain and every row
                // fell back to hugging the longest tab name. Border-box takes
                // the rail's 1px right border off the content width.
                v_flex()
                    .w(px(width - 1.))
                    .h_full()
                    .child(crate::ui::app::title_bar_drag(
                        controls.id("sidebar-titlebar-drag"),
                        "sidebar-titlebar-drag",
                        window,
                        cx,
                    ))
                    .child(workspace_head)
                    .child(top_bar)
                    .child(crate::ui::scrollbar::with_vertical_scrollbar(
                        "tab-sidebar-scrollbar",
                        list,
                        &self.sidebar_scroll,
                    )),
            )
            .child(handle)
    }

    /// What the sidebar row hid: the full title, the full branch and diff
    /// counts, the working directory, and the target behind the SSH marker.
    /// `None` when the row showed everything — a card would add noise,
    /// not information. The host is included even for an untruncated row,
    /// because the title strips the `user@host:` prefix the avatar cannot
    /// spell out.
    ///
    /// Every line is decided by comparing what the row rendered against the
    /// string it was elided from. Both come from the row itself: deriving
    /// them here a second time is how a renamed tab ended up with a name the
    /// row shortened and the card refused to expand.
    fn sidebar_info(
        &self,
        tab: &crate::ui::app::Tab,
        window: &mut Window,
        cx: &gpui::App,
        shown: &SidebarRowShown,
    ) -> Option<SidebarInfo> {
        let elided = |pair: &Option<(SharedString, SharedString)>| {
            pair.as_ref()
                .filter(|(shown, full)| shown != full)
                .map(|(_, full)| full.clone())
        };
        let mut info = SidebarInfo {
            title: elided(&shown.title),
            branch: shown
                .branch
                .as_ref()
                .filter(|(shown, full, _, _)| shown != full)
                .map(|(_, full, added, removed)| (full.clone(), *added, *removed)),
            // The cwd only earns a card line when it was rendered *and*
            // elided: a repo row already shows the full path as its title, so
            // repeating the cwd under it would be noise, not information.
            cwd: elided(&shown.cwd),
            host: None,
        };
        // The host is read off the same leaf the title and cwd came from; a
        // split tab whose panes sit on different machines would otherwise
        // name whichever one happens to be first.
        if let Some(target) = tab
            .title_details(Some(window), cx)
            .ssh_target
            .or_else(|| tab.remote_context(Some(window), cx).map(|r| r.target))
            .map(SharedString::from)
        {
            info.host = Some(target);
        }
        (info.title.is_some() || info.branch.is_some() || info.cwd.is_some() || info.host.is_some())
            .then_some(info)
    }

    fn sidebar_group_keys(&self, cx: &gpui::App) -> Vec<Option<PathBuf>> {
        let grouping = cx.global::<Config>().sidebar_grouping;
        self.tabs
            .iter()
            .map(|tab| {
                if grouping == SidebarGrouping::None {
                    return None;
                }
                let cwd = tab.pane.first_leaf().and_then(|leaf| {
                    let view = leaf.terminal()?.read(cx);
                    let remote = view.remote_context();
                    if let Some(remote) =
                        remote.filter(|r| in_pane_ssh_hop(view.host_id().is_local(), Some(r.kind)))
                    {
                        // A hop is grouped by where it went, not by the
                        // local directory it left; the display identity
                        // keeps profile names readable and explicit login
                        // and port options distinct.
                        *tab.sidebar_group.borrow_mut() =
                            Some(ssh_hop_group_key(&remote.display_target()));
                        return None;
                    }
                    Some((view.git_host_id(), view.git_status_cwd()?.to_path_buf()))
                });
                if let Some((id, cwd)) = cwd {
                    let known = cx.global::<GitStatusCache>().known_repo_for(id, &cwd);
                    if let Some(group) = resolved_group(grouping, known, &cwd) {
                        *tab.sidebar_group.borrow_mut() = group;
                    }
                }
                tab.sidebar_group.borrow().clone()
            })
            .collect()
    }

    fn visual_tab_order(&self, cx: &gpui::App) -> Vec<usize> {
        if cx.global::<Config>().tab_bar_position != crate::core::config::TabBarPosition::Left {
            return (0..self.tabs.len()).collect();
        }
        let keys = self.sidebar_group_keys(cx);
        self.sidebar_sections(&keys, cx)
            .into_iter()
            .flat_map(|s| s.tabs)
            .collect()
    }

    fn sidebar_pinned_groups<'a>(&self, cx: &'a gpui::App) -> &'a [String] {
        if self.can_spawn_locally(cx) {
            &cx.global::<Config>().sidebar_pinned_groups
        } else {
            &[]
        }
    }

    fn sidebar_sections(&self, keys: &[Option<PathBuf>], cx: &gpui::App) -> Vec<Section> {
        let pinned = self.sidebar_pinned_groups(cx);
        let cache = self.pinned_dir_cache.borrow();
        sidebar_sections(keys, pinned, &|p| cache.get(p) == Some(&true))
    }

    fn probe_pinned_dirs(
        &self,
        keys: &[Option<PathBuf>],
        pinned: &[String],
        cx: &mut Context<Self>,
    ) {
        let Some(local_host) =
            crate::ui::host_registry::HostRegistry::lookup(cx, crate::ui::host_ops::HostId::LOCAL)
        else {
            return;
        };
        for p_str in pinned {
            if p_str.is_empty() {
                continue;
            }
            let path = PathBuf::from(p_str);
            if keys.iter().flatten().any(|k| k == &path) {
                continue;
            }
            if self.pinned_dir_cache.borrow().contains_key(&path) {
                continue;
            }
            if !self.pinned_dir_probes.borrow_mut().begin(path.clone()) {
                continue;
            }
            let probe_path = path.clone();
            crate::ui::host_ops::HostOps::run(
                local_host.clone(),
                cx,
                move |h| h.stat(&probe_path).map(|m| m.is_dir).unwrap_or(false),
                move |this, is_dir, cx| {
                    if this.pinned_dir_probes.borrow_mut().finish(&path) {
                        this.pinned_dir_cache.borrow_mut().insert(path, is_dir);
                    }
                    cx.notify();
                },
            );
        }
    }

    fn toggle_group_collapsed(&mut self, key: &str, cx: &mut Context<Self>) {
        self.update_config(cx, |cfg| {
            if !cfg.sidebar_collapsed_groups.remove(key) {
                cfg.sidebar_collapsed_groups.insert(key.to_string());
            }
        });
    }

    pub(crate) fn unfold_group(&mut self, key: &str, cx: &mut Context<Self>) {
        if cx.global::<Config>().sidebar_collapsed_groups.contains(key) {
            self.update_config(cx, |cfg| {
                cfg.sidebar_collapsed_groups.remove(key);
            });
        }
    }

    fn collapse_other_groups(&mut self, keep: &str, all_headed: &[String], cx: &mut Context<Self>) {
        self.update_config(cx, |cfg| {
            cfg.sidebar_collapsed_groups.remove(keep);
            for k in all_headed {
                if k != keep {
                    cfg.sidebar_collapsed_groups.insert(k.clone());
                }
            }
        });
    }

    fn set_all_groups_collapsed(
        &mut self,
        all_headed: &[String],
        collapse: bool,
        cx: &mut Context<Self>,
    ) {
        self.update_config(cx, |cfg| {
            for k in all_headed {
                if collapse {
                    cfg.sidebar_collapsed_groups.insert(k.clone());
                } else {
                    cfg.sidebar_collapsed_groups.remove(k);
                }
            }
        });
    }

    fn toggle_pin_group(&mut self, key: &str, cx: &mut Context<Self>) {
        // Pinning probes the key as a directory; an SSH group has none.
        if key.is_empty() || ssh_hop_group_label(key).is_some() || !self.can_spawn_locally(cx) {
            return;
        }
        let path = PathBuf::from(key);
        self.pinned_dir_cache.borrow_mut().remove(&path);
        self.pinned_dir_probes.borrow_mut().invalidate(&path);
        self.update_config(cx, |cfg| {
            if let Some(pos) = cfg.sidebar_pinned_groups.iter().position(|k| k == key) {
                cfg.sidebar_pinned_groups.remove(pos);
            } else {
                cfg.sidebar_pinned_groups.push(key.to_string());
            }
        });
    }

    fn new_tab_in_group(
        &mut self,
        key: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A local spawn has no meaning on a remote host's group.
        if key.as_deref().and_then(ssh_hop_group_target).is_some() {
            return;
        }
        self.unfold_group(&group_name_key(&key), cx);
        match key {
            Some(path) => self.new_tab_at(path, window, cx),
            None => self.new_tab(window, cx),
        }
    }

    fn group_header_menu(
        mut menu: PopupMenu,
        app: &gpui::WeakEntity<Self>,
        key: Option<PathBuf>,
        current_name: String,
        is_pinned: bool,
        is_local: bool,
        all_headed: Vec<String>,
    ) -> PopupMenu {
        menu = menu.min_w(px(160.));
        let key_str = group_name_key(&key);
        let is_ssh_group = key.as_deref().and_then(ssh_hop_group_target).is_some();
        if is_local && key.is_some() && !is_ssh_group {
            let pin_label = if is_pinned {
                t(L10nKey::SidebarUnpinGroup)
            } else {
                t(L10nKey::SidebarPinGroup)
            };
            let app_pin = app.clone();
            let pin_key = key_str.clone();
            menu = menu.item(
                PopupMenuItem::new(pin_label).on_click(move |_, _window, cx| {
                    let _ = app_pin.update(cx, |this, cx| {
                        this.toggle_pin_group(&pin_key, cx);
                    });
                }),
            );
        }
        if !is_ssh_group {
            let app_new = app.clone();
            let new_key = key.clone();
            menu = menu.item(PopupMenuItem::new(t(L10nKey::SidebarNewTabHere)).on_click(
                move |_, window, cx| {
                    let _ = app_new.update(cx, |this, cx| {
                        this.new_tab_in_group(new_key.clone(), window, cx);
                    });
                },
            ));
        }
        let app_rename = app.clone();
        let rename_key = key.clone();
        menu = menu
            .item(PopupMenuItem::new(t(L10nKey::SidebarRenameGroup)).on_click(
                move |_, window, cx| {
                    let _ = app_rename.update(cx, |this, cx| {
                        this.start_group_rename(
                            rename_key.clone(),
                            current_name.clone(),
                            window,
                            cx,
                        )
                    });
                },
            ))
            .separator();

        let app_others = app.clone();
        let others_keep = key_str.clone();
        let others_headed = all_headed.clone();
        let app_collapse = app.clone();
        let collapse_headed = all_headed.clone();
        let app_expand = app.clone();
        let expand_headed = all_headed;
        menu.item(
            PopupMenuItem::new(t(L10nKey::SidebarCollapseOthers)).on_click(
                move |_, _window, cx| {
                    let _ = app_others.update(cx, |this, cx| {
                        this.collapse_other_groups(&others_keep, &others_headed, cx);
                    });
                },
            ),
        )
        .item(
            PopupMenuItem::new(t(L10nKey::SidebarCollapseAll)).on_click(move |_, _window, cx| {
                let _ = app_collapse.update(cx, |this, cx| {
                    this.set_all_groups_collapsed(&collapse_headed, true, cx);
                });
            }),
        )
        .item(
            PopupMenuItem::new(t(L10nKey::SidebarExpandAll)).on_click(move |_, _window, cx| {
                let _ = app_expand.update(cx, |this, cx| {
                    this.set_all_groups_collapsed(&expand_headed, false, cx);
                });
            }),
        )
    }

    pub(crate) fn activate_visual(
        &mut self,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(&i) = self.visual_tab_order(cx).get(n) {
            self.activate(i, window, cx);
        }
    }

    /// Which group a tab about to be spawned in `cwd` belongs to, when the
    /// repo probe for that directory has already landed. A bare `None` means
    /// the cache never looked; `Some` is the group [`resolved_group`] reached
    /// — the repo home, the cwd itself under repo-or-directory grouping, or
    /// `Some(None)` for Scratch.
    ///
    /// A tab's group otherwise starts empty and only fills in once its shell
    /// has started and reported a cwd, which parks every new tab in the
    /// scratch group at the bottom of the sidebar until then. A tab spawned
    /// from one already sitting in a repo inherits a warm cache, so seeding
    /// it here lands the tab in its group on the first frame.
    pub(crate) fn spawn_group(
        &self,
        cwd: Option<&Path>,
        cx: &gpui::App,
    ) -> Option<Option<PathBuf>> {
        let cwd = cwd?;
        let host = self
            .window_workspace(cx)
            .as_ref()
            .map_or(crate::ui::host_ops::HostId::LOCAL, |ws| ws.target.host_id());
        let known = cx.try_global::<GitStatusCache>()?.known_repo_for(host, cwd);
        resolved_group(cx.global::<Config>().sidebar_grouping, known, cwd)
    }
}

/// Scratch has no directory icon. When the arrow is hidden, the folder itself
/// indicates the expanded or collapsed state.
fn group_folder_icon(
    is_project: bool,
    show_folder: bool,
    show_arrow: bool,
    collapsed: bool,
) -> Option<&'static str> {
    if !is_project || !show_folder {
        None
    } else if show_arrow {
        Some("icons/folder.svg")
    } else if collapsed {
        Some("icons/folder-closed.svg")
    } else {
        Some("icons/folder-open.svg")
    }
}

/// Scratch has no folder, so with folder icons enabled and arrows hidden the
/// arrow stays: it is the only state cue the Scratch header can show. When
/// both icon styles are hidden no header shows an arrow.
fn header_shows_chevron(is_project: bool, show_arrow: bool, show_folder: bool) -> bool {
    show_arrow || (!is_project && show_folder)
}

/// A hop this pane made with `ssh` (or tty7's own SSH engine) from a local
/// window. The far side is not a Host we can git-probe, so the tab must not
/// keep the local repo group it started in.
fn in_pane_ssh_hop(host_is_local: bool, kind: Option<RemoteKind>) -> bool {
    host_is_local && matches!(kind, Some(RemoteKind::Ssh | RemoteKind::NativeSsh))
}

/// Group keys for in-pane SSH hops: `ssh:` + the connection's display identity
/// (profile name, alias, or the address as typed). Every real group root is an
/// absolute path, so a prefixed key — always relative — cannot collide with
/// one. The rest of the config surface ([`group_name_key`], folded groups,
/// custom titles) carries these keys as opaque strings.
const SSH_GROUP_PREFIX: &str = "ssh:";

/// The target behind an `ssh:`-prefixed key string, or `None` for a path key.
fn ssh_hop_group_label(key: &str) -> Option<&str> {
    key.strip_prefix(SSH_GROUP_PREFIX)
}

/// The group key for an in-pane SSH hop to `target`.
fn ssh_hop_group_key(target: &str) -> PathBuf {
    PathBuf::from(format!("{SSH_GROUP_PREFIX}{target}"))
}

/// The SSH target behind a group key, or `None` for a path group.
fn ssh_hop_group_target(key: &Path) -> Option<&str> {
    ssh_hop_group_label(key.to_str()?)
}

/// The group a probed cwd resolves to under `grouping`: the repo home when
/// the cache found one, otherwise Scratch — or the cwd itself under
/// repo-or-directory grouping, so a shell in a plain folder still gets a
/// header. `known` is the cache's three-valued answer; a probe that never
/// ran resolves to `None`, no decision, and the tab keeps whatever group it
/// already has rather than bouncing through Scratch mid-probe.
pub(crate) fn resolved_group(
    grouping: SidebarGrouping,
    known: Option<Option<PathBuf>>,
    cwd: &Path,
) -> Option<Option<PathBuf>> {
    Some(match known? {
        Some(root) => Some(root),
        None if grouping == SidebarGrouping::RepoOrDirectory => Some(cwd.to_path_buf()),
        None => None,
    })
}

/// Cwd a path-open reveal should ask the repo cache about.
///
/// A shell that has not reported yet still answers with the directory the
/// open asked for. An in-pane `ssh` hop is not that directory — the tab has
/// left it — so the caller waits rather than unfolding the local folder.
pub(crate) fn probed_group_cwd(
    tab: &crate::ui::app::Tab,
    stored: &Path,
    cx: &gpui::App,
) -> Option<(crate::ui::host_ops::HostId, PathBuf)> {
    if let Some(leaf) = tab.pane.first_leaf() {
        if let Some(view) = leaf.terminal() {
            let view = view.read(cx);
            if in_pane_ssh_hop(
                view.host_id().is_local(),
                view.remote_context().map(|r| r.kind),
            ) {
                return None;
            }
            if let Some(cwd) = view.git_status_cwd() {
                return Some((view.git_host_id(), cwd.to_path_buf()));
            }
        }
    }
    Some((crate::ui::host_ops::HostId::LOCAL, stored.to_path_buf()))
}

#[derive(Debug, PartialEq)]
struct Section {
    key: Option<PathBuf>,
    name: Option<String>,
    tabs: Vec<usize>,
    pinned: bool,
}

fn sidebar_sections(
    keys: &[Option<PathBuf>],
    pinned: &[String],
    keep_empty: &dyn Fn(&Path) -> bool,
) -> Vec<Section> {
    let mut active_groups: Vec<PathBuf> = Vec::new();
    for k in keys.iter().flatten() {
        if !active_groups.contains(k) {
            active_groups.push(k.clone());
        }
    }
    let mut pinned_order: Vec<PathBuf> = Vec::new();
    for p_str in pinned {
        // `""` is Scratch, and an SSH key is never pinnable: pinning probes
        // the root as a directory, and a hop's host has no local root.
        if p_str.is_empty() || ssh_hop_group_label(p_str).is_some() {
            continue;
        }
        let path = PathBuf::from(p_str);
        if (active_groups.contains(&path) || keep_empty(&path)) && !pinned_order.contains(&path) {
            pinned_order.push(path);
        }
    }
    let mut unpinned_order: Vec<PathBuf> = active_groups
        .into_iter()
        .filter(|g| !pinned_order.contains(g))
        .collect();
    // SSH groups park between the local project groups and Scratch: a hop's
    // host is a destination, not a local root, and Scratch stays the
    // catch-all at the bottom. Within each kind, first appearance decides.
    let ssh_groups: Vec<PathBuf> = unpinned_order
        .iter()
        .filter(|g| ssh_hop_group_target(g.as_path()).is_some())
        .cloned()
        .collect();
    unpinned_order.retain(|g| ssh_hop_group_target(g.as_path()).is_none());
    unpinned_order.extend(ssh_groups);
    if pinned_order.is_empty() && unpinned_order.is_empty() {
        return vec![Section {
            key: None,
            name: None,
            tabs: (0..keys.len()).collect(),
            pinned: false,
        }];
    }
    let pinned_count = pinned_order.len();
    let mut all_groups = Vec::with_capacity(pinned_count + unpinned_order.len());
    all_groups.extend(pinned_order);
    all_groups.extend(unpinned_order);
    let path_roots: Vec<&PathBuf> = all_groups
        .iter()
        .filter(|g| ssh_hop_group_target(g.as_path()).is_none())
        .collect();
    let mut derived = group_names(&path_roots).into_iter();
    let names: Vec<String> = all_groups
        .iter()
        .map(|root| match ssh_hop_group_target(root.as_path()) {
            Some(target) => target.to_string(),
            None => derived.next().expect("one derived name per path group"),
        })
        .collect();
    let mut sections: Vec<Section> = all_groups
        .into_iter()
        .zip(names)
        .enumerate()
        .map(|(idx, (root, name))| {
            let tabs = (0..keys.len())
                .filter(|&i| keys[i].as_ref() == Some(&root))
                .collect();
            Section {
                key: Some(root),
                name: Some(name),
                tabs,
                pinned: idx < pinned_count,
            }
        })
        .collect();
    let scratch: Vec<usize> = (0..keys.len()).filter(|&i| keys[i].is_none()).collect();
    if !scratch.is_empty() {
        sections.push(Section {
            key: None,
            name: Some(t(L10nKey::SidebarScratchGroup).to_string()),
            tabs: scratch,
            pinned: false,
        });
    }
    sections
}

/// The key a group's custom title is stored under in
/// [`Config::sidebar_group_names`]: the group's root path, with `""` standing
/// for the Scratch group.
pub(crate) fn group_name_key(key: &Option<PathBuf>) -> String {
    key.as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What a group header shows: the user's custom title when one is stored,
/// otherwise the name derived from the group's path.
fn effective_group_name(cfg: &Config, key: &Option<PathBuf>, derived: String) -> String {
    cfg.sidebar_group_names
        .get(&group_name_key(key))
        .cloned()
        .unwrap_or(derived)
}

fn reordered_rows(
    keys: &[Option<PathBuf>],
    pinned: &[String],
    keep_empty: &dyn Fn(&Path) -> bool,
    group: &Option<PathBuf>,
    visible: &[usize],
    from: usize,
    to: usize,
) -> Option<Vec<usize>> {
    let (&moved, &anchor) = (visible.get(from)?, visible.get(to)?);
    if moved == anchor {
        return None;
    }
    let mut members: Vec<usize> = (0..keys.len()).filter(|&i| keys[i] == *group).collect();
    members.retain(|&i| i != moved);
    let at = members.iter().position(|&i| i == anchor)? + usize::from(to > from);
    members.insert(at, moved);

    let mut out: Vec<usize> = Vec::with_capacity(keys.len());
    for g in sidebar_sections(keys, pinned, keep_empty)
        .iter()
        .map(|s| &s.key)
    {
        if g == group {
            out.extend_from_slice(&members);
        } else {
            out.extend((0..keys.len()).filter(|&i| keys[i] == *g));
        }
    }
    Some(out)
}

fn reordered_pinned(pinned: &[String], from: &Path, to: &Path) -> Option<Vec<String>> {
    if from == to {
        return None;
    }
    let from_str = from.to_string_lossy();
    let to_str = to.to_string_lossy();
    let mut order: Vec<String> = pinned.to_vec();
    let fi = order.iter().position(|k| k == from_str.as_ref())?;
    let ti = order.iter().position(|k| k == to_str.as_ref())?;
    let moved = order.remove(fi);
    order.insert(ti, moved);
    Some(order)
}

fn regrouped_order(keys: &[Option<PathBuf>], from: &Path, to: &Path) -> Option<Vec<usize>> {
    if from == to {
        return None;
    }
    let mut order: Vec<&PathBuf> = Vec::new();
    for k in keys.iter().flatten() {
        if !order.iter().any(|g| *g == k) {
            order.push(k);
        }
    }
    let fi = order.iter().position(|g| g.as_path() == from)?;
    let ti = order.iter().position(|g| g.as_path() == to)?;
    let moved = order.remove(fi);
    order.insert(ti, moved);

    let mut out: Vec<usize> = Vec::with_capacity(keys.len());
    for g in &order {
        out.extend((0..keys.len()).filter(|&i| keys[i].as_ref() == Some(*g)));
    }
    out.extend((0..keys.len()).filter(|&i| keys[i].is_none()));
    Some(out)
}

fn group_names(roots: &[&PathBuf]) -> Vec<String> {
    let comps: Vec<Vec<String>> = roots
        .iter()
        .map(|r| {
            r.components()
                .filter(|c| matches!(c, std::path::Component::Normal(_)))
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect()
        })
        .collect();
    let mut depth = vec![1usize; roots.len()];
    loop {
        let names: Vec<String> = comps
            .iter()
            .zip(&depth)
            .enumerate()
            .map(|(i, (c, &d))| {
                if c.is_empty() {
                    roots[i].display().to_string()
                } else {
                    c[c.len().saturating_sub(d)..].join("/")
                }
            })
            .collect();
        let mut grew = false;
        for i in 0..names.len() {
            let collides = names
                .iter()
                .enumerate()
                .any(|(j, n)| j != i && *n == names[i]);
            if collides && depth[i] < comps[i].len() {
                depth[i] += 1;
                grew = true;
            }
        }
        if !grew {
            return names;
        }
    }
}

/// Whether a `+N −M` is a button, and what it opens if it is.
///
/// One function because the setting is one setting: the sidebar's counts and
/// the Info panel's `changes` row are the same number about the same working
/// tree, and "Open diff preview from sidebar counts" turning one of them into
/// plain text while the other stayed clickable would be a setting that half
/// works.
pub(crate) fn diff_click_cwd<T>(cfg: &Config, target: Option<T>) -> Option<T> {
    cfg.sidebar_diff_preview.then_some(target).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn group_folder_icon_follows_the_display_choices_and_group_state() {
        for arrow in [false, true] {
            for collapsed in [false, true] {
                assert_eq!(group_folder_icon(true, false, arrow, collapsed), None);
                assert_eq!(group_folder_icon(false, true, arrow, collapsed), None);
            }
        }
        assert_eq!(
            group_folder_icon(true, true, true, false),
            Some("icons/folder.svg")
        );
        assert_eq!(
            group_folder_icon(true, true, true, true),
            Some("icons/folder.svg")
        );
        assert_eq!(
            group_folder_icon(true, true, false, false),
            Some("icons/folder-open.svg")
        );
        assert_eq!(
            group_folder_icon(true, true, false, true),
            Some("icons/folder-closed.svg")
        );
    }

    #[test]
    fn scratch_keeps_its_arrow_when_only_folder_icons_are_shown() {
        // Projects swap the arrow for the folder when arrows are hidden.
        assert!(header_shows_chevron(true, true, false));
        assert!(header_shows_chevron(true, true, true));
        assert!(!header_shows_chevron(true, false, true));
        assert!(!header_shows_chevron(true, false, false));
        // Scratch has no folder, so it keeps the arrow while folders are on.
        assert!(header_shows_chevron(false, true, false));
        assert!(header_shows_chevron(false, true, true));
        assert!(header_shows_chevron(false, false, true));
        assert!(!header_shows_chevron(false, false, false));
    }

    #[gpui::test]
    fn ssh_group_icons_follow_the_same_settings_as_project_groups(cx: &mut gpui::TestAppContext) {
        use crate::ui::app::{Tab, test_window::harness};
        use crate::ui::pane::Pane;

        let (app, mut vcx) = harness(cx);
        vcx.simulate_resize(size(px(800.), px(600.)));
        let ssh_key = ssh_hop_group_key("dev-box");
        app.update(&mut vcx, |app, cx| {
            cx.global_mut::<Config>().sidebar_grouping = SidebarGrouping::Repo;
            app.tabs = [p("/w/project"), ssh_key.clone()]
                .into_iter()
                .map(|group| {
                    let tab = Tab::new(Pane::Empty);
                    *tab.sidebar_group.borrow_mut() = Some(group);
                    tab
                })
                .collect();
            app.active = 0;
            cx.notify();
        });

        for collapsed in [false, true] {
            for (arrows, folders) in [(false, true), (true, false), (true, true), (false, false)] {
                app.update(&mut vcx, |_, cx| {
                    let cfg = cx.global_mut::<Config>();
                    cfg.sidebar_group_arrows = arrows;
                    cfg.sidebar_group_folders = folders;
                    cfg.sidebar_collapsed_groups = if collapsed {
                        ["/w/project".into(), ssh_key.to_string_lossy().into_owned()]
                            .into_iter()
                            .collect()
                    } else {
                        Default::default()
                    };
                    cx.notify();
                });
                vcx.run_until_parked();
                for (group, chevron, folder) in [
                    (0, "sidebar-group-chevron-0", "sidebar-group-folder-0"),
                    (1, "sidebar-group-chevron-1", "sidebar-group-folder-1"),
                ] {
                    assert_eq!(
                        vcx.debug_bounds(chevron).is_some(),
                        arrows,
                        "group={group}, collapsed={collapsed}, arrows={arrows}, folders={folders}"
                    );
                    assert_eq!(
                        vcx.debug_bounds(folder).is_some(),
                        folders,
                        "group={group}, collapsed={collapsed}, arrows={arrows}, folders={folders}"
                    );
                }
            }
        }
    }

    #[gpui::test]
    fn leading_icons_share_the_workspace_axis(cx: &mut gpui::TestAppContext) {
        use crate::ui::app::{Tab, test_window::harness};
        use crate::ui::pane::Pane;

        let (app, mut vcx) = harness(cx);
        vcx.simulate_resize(size(px(800.), px(600.)));
        app.update(&mut vcx, |app, cx| {
            cx.global_mut::<Config>().sidebar_grouping = SidebarGrouping::Repo;
            let tab = Tab::new(Pane::Empty);
            *tab.sidebar_group.borrow_mut() = Some(p("/w/project"));
            app.tabs = vec![tab, Tab::new(Pane::Empty)];
            app.active = 0;
            cx.notify();
        });

        for (arrows, folders) in [(true, false), (false, true), (true, true)] {
            app.update(&mut vcx, |_, cx| {
                let cfg = cx.global_mut::<Config>();
                cfg.sidebar_group_arrows = arrows;
                cfg.sidebar_group_folders = folders;
                cx.notify();
            });
            vcx.run_until_parked();
            let axis = vcx
                .debug_bounds("workspace-head-avatar")
                .unwrap()
                .center()
                .x;
            let search = vcx.debug_bounds("sidebar-search-icon").unwrap().center().x;
            assert_eq!(
                search, axis,
                "workspace/search: arrows={arrows}, folders={folders}"
            );
            let first = if arrows {
                "sidebar-group-chevron-0"
            } else {
                "sidebar-group-folder-0"
            };
            assert_eq!(
                vcx.debug_bounds(first).unwrap().center().x,
                axis,
                "group: arrows={arrows}, folders={folders}"
            );
            assert_eq!(
                vcx.debug_bounds("sidebar-group-chevron-1")
                    .unwrap()
                    .center()
                    .x,
                axis,
                "Scratch: arrows={arrows}, folders={folders}"
            );
        }
    }

    #[gpui::test]
    fn grouped_rows_indent_their_contents_but_keep_the_full_click_target(
        cx: &mut gpui::TestAppContext,
    ) {
        use crate::ui::app::{Tab, test_window::harness};
        use crate::ui::pane::Pane;

        let (app, mut vcx) = harness(cx);
        vcx.simulate_resize(size(px(800.), px(600.)));
        app.update(&mut vcx, |app, cx| {
            cx.global_mut::<Config>().sidebar_grouping = SidebarGrouping::Repo;
            app.tabs = [Some(p("/w/project")), Some(p("/w/project")), None]
                .into_iter()
                .map(|group| {
                    let tab = Tab::new(Pane::Empty);
                    *tab.sidebar_group.borrow_mut() = group;
                    tab
                })
                .collect();
            app.active = 0;
            cx.notify();
        });
        vcx.run_until_parked();

        let grouped_row = vcx.debug_bounds("sidebar-tab-row-0").unwrap();
        let grouped_project = vcx.debug_bounds("sidebar-tab-avatar-0").unwrap();
        let grouped_scratch = vcx.debug_bounds("sidebar-tab-avatar-2").unwrap();
        let second_row = vcx.debug_bounds("sidebar-tab-row-1").unwrap();
        vcx.simulate_click(
            point(second_row.left() + px(2.), second_row.center().y),
            gpui::Modifiers::none(),
        );
        vcx.run_until_parked();
        assert_eq!(app.read_with(&vcx, |app, _| app.active), 1);

        app.update(&mut vcx, |_, cx| {
            cx.global_mut::<Config>().sidebar_grouping = SidebarGrouping::None;
            cx.notify();
        });
        vcx.run_until_parked();
        let flat_row = vcx.debug_bounds("sidebar-tab-row-0").unwrap();
        let flat_project = vcx.debug_bounds("sidebar-tab-avatar-0").unwrap();
        let flat_scratch = vcx.debug_bounds("sidebar-tab-avatar-2").unwrap();
        assert_eq!(grouped_row.left(), flat_row.left());
        assert_eq!(grouped_row.size.width, flat_row.size.width);
        assert_eq!(
            grouped_project.left() - flat_project.left(),
            px(row_metrics::GROUP_INDENT)
        );
        assert_eq!(
            grouped_scratch.left() - flat_scratch.left(),
            px(row_metrics::GROUP_INDENT)
        );
    }

    #[gpui::test]
    fn remote_windows_ignore_local_pins_and_cannot_change_them(cx: &mut gpui::TestAppContext) {
        use crate::core::session::{
            RemoteRef, RemoteTarget, WindowView, WindowViews, WorkspaceId, WorkspaceStore,
        };
        use crate::ui::app::{Tab, test_window::harness};
        use crate::ui::pane::Pane;

        let (app, mut vcx) = harness(cx);
        app.update(&mut vcx, |app, cx| {
            let pinned = vec!["/w/alpha".to_string(), "/w/empty".to_string()];
            let cfg = cx.global_mut::<Config>();
            cfg.tab_bar_position = crate::core::config::TabBarPosition::Left;
            cfg.sidebar_pinned_groups = pinned.clone();
            app.tabs = ["/w/beta", "/w/alpha"]
                .map(|key| {
                    let tab = Tab::new(Pane::Empty);
                    *tab.sidebar_group.borrow_mut() = Some(p(key));
                    tab
                })
                .into();
            app.pinned_dir_cache
                .borrow_mut()
                .insert(p("/w/empty"), true);
            assert_eq!(app.visual_tab_order(cx), vec![1, 0]);

            let remote = WindowView::on_remote(RemoteRef::new(
                RemoteTarget::Alias {
                    alias: "build-box".into(),
                },
                WorkspaceId::new(),
            ));
            app.workspace = remote.id;
            WorkspaceStore::install_for_test(
                cx,
                WindowViews {
                    views: vec![remote],
                    active: None,
                },
            );

            assert_eq!(app.visual_tab_order(cx), vec![0, 1]);
            let sections = app.sidebar_sections(&app.sidebar_group_keys(cx), cx);
            assert_eq!(sections.len(), 2, "local empty pins stay hidden remotely");
            assert!(sections.iter().all(|section| !section.pinned));
            app.toggle_pin_group("/w/remote", cx);
            app.toggle_pin_group("/w/alpha", cx);
            assert_eq!(cx.global::<Config>().sidebar_pinned_groups, pinned);
        });
    }

    #[test]
    fn custom_group_titles_override_derived_names_by_key() {
        let mut cfg = Config::default();
        let repo = Some(p("/w/tty7"));
        assert_eq!(
            effective_group_name(&cfg, &repo, "tty7".into()),
            "tty7",
            "no entry: the derived name stands"
        );

        cfg.sidebar_group_names
            .insert(group_name_key(&repo), "mine".into());
        cfg.sidebar_group_names
            .insert(group_name_key(&None), "inbox".into());
        assert_eq!(effective_group_name(&cfg, &repo, "tty7".into()), "mine");
        assert_eq!(
            effective_group_name(&cfg, &None, "Scratch".into()),
            "inbox",
            "the scratch group renames through the \"\" key"
        );
        assert_eq!(
            effective_group_name(&cfg, &Some(p("/w/other")), "other".into()),
            "other",
            "an entry only covers its own group"
        );
    }

    #[test]
    fn diff_preview_setting_gates_the_click_target() {
        let mut cfg = Config::default();
        assert!(cfg.sidebar_diff_preview, "default is today's behaviour");
        assert_eq!(
            diff_click_cwd(&cfg, Some(p("/w/repo"))),
            Some(p("/w/repo")),
            "enabled: the counts are a click target"
        );

        cfg.sidebar_diff_preview = false;
        assert_eq!(
            diff_click_cwd(&cfg, Some(p("/w/repo"))),
            None,
            "disabled: no cwd, so no cursor and no toggle_diff_overlay"
        );
    }

    #[test]
    fn diff_click_target_needs_a_repo_either_way() {
        let mut cfg = Config::default();
        assert_eq!(diff_click_cwd::<PathBuf>(&cfg, None), None);
        cfg.sidebar_diff_preview = false;
        assert_eq!(diff_click_cwd::<PathBuf>(&cfg, None), None);
    }

    #[test]
    fn a_probed_non_repo_groups_by_folder_only_in_the_fallback_mode() {
        let cwd = p("/w/plain");
        assert_eq!(
            resolved_group(SidebarGrouping::RepoOrDirectory, Some(None), &cwd),
            Some(Some(p("/w/plain")))
        );
        assert_eq!(
            resolved_group(SidebarGrouping::Repo, Some(None), &cwd),
            Some(None),
            "under Repo a probed non-repo still falls to Scratch"
        );
        // Never probed: no decision in either mode, so the tab keeps the
        // group it already has instead of bouncing through Scratch.
        assert_eq!(resolved_group(SidebarGrouping::Repo, None, &cwd), None);
        assert_eq!(
            resolved_group(SidebarGrouping::RepoOrDirectory, None, &cwd),
            None
        );
    }

    #[test]
    fn a_known_repo_home_wins_over_the_folder_in_both_modes() {
        for mode in [SidebarGrouping::Repo, SidebarGrouping::RepoOrDirectory] {
            assert_eq!(
                resolved_group(mode, Some(Some(p("/w/repo"))), &p("/w/repo/sub")),
                Some(Some(p("/w/repo")))
            );
        }
    }

    #[test]
    fn an_in_pane_ssh_hop_is_not_the_local_repo() {
        assert!(in_pane_ssh_hop(true, Some(RemoteKind::Ssh)));
        assert!(in_pane_ssh_hop(true, Some(RemoteKind::NativeSsh)));
        assert!(
            !in_pane_ssh_hop(true, Some(RemoteKind::Wsl)),
            "WSL still has a path this machine can read"
        );
        assert!(!in_pane_ssh_hop(true, None));
        assert!(
            !in_pane_ssh_hop(false, Some(RemoteKind::Ssh)),
            "a remote workspace probes git on that host"
        );
    }

    #[test]
    fn an_ssh_group_key_round_trips_its_target() {
        let key = ssh_hop_group_key("deploy@prod-web");
        assert_eq!(ssh_hop_group_target(&key), Some("deploy@prod-web"));
        assert_eq!(group_name_key(&Some(key)), "ssh:deploy@prod-web");
        assert!(ssh_hop_group_target(&p("/w/repo")).is_none());
    }

    #[test]
    fn sections_place_ssh_groups_between_local_groups_and_scratch() {
        let keys = vec![
            Some(p("/w/beta")),
            None,
            Some(ssh_hop_group_key("dev-box")),
            Some(p("/w/alpha")),
            Some(ssh_hop_group_key("prod-web")),
            Some(ssh_hop_group_key("dev-box")),
        ];
        let sections = sidebar_sections(&keys, &[], &|_| true);
        let shape: Vec<(Option<PathBuf>, Option<String>, Vec<usize>)> = sections
            .into_iter()
            .map(|s| (s.key, s.name, s.tabs))
            .collect();
        assert_eq!(
            shape,
            vec![
                (Some(p("/w/beta")), Some("beta".into()), vec![0]),
                (Some(p("/w/alpha")), Some("alpha".into()), vec![3]),
                (
                    Some(ssh_hop_group_key("dev-box")),
                    Some("dev-box".into()),
                    vec![2, 5]
                ),
                (
                    Some(ssh_hop_group_key("prod-web")),
                    Some("prod-web".into()),
                    vec![4]
                ),
                (None, Some("Scratch".into()), vec![1]),
            ]
        );

        // A stale SSH key hand-edited into the pin list must not pin
        // anything: the pinned probe would treat it as a directory.
        let pinned = vec![ssh_hop_group_key("dev-box").to_string_lossy().into_owned()];
        let sections = sidebar_sections(&keys, &pinned, &|_| true);
        assert!(sections.iter().all(|section| !section.pinned));
    }

    #[gpui::test]
    fn ssh_groups_cannot_be_pinned(cx: &mut gpui::TestAppContext) {
        use crate::ui::app::test_window::harness;

        let (app, mut vcx) = harness(cx);
        app.update(&mut vcx, |app, cx| {
            app.toggle_pin_group("ssh:dev-box", cx);
            assert!(
                cx.global::<Config>().sidebar_pinned_groups.is_empty(),
                "the SSH group guard refuses the pin before it reaches config"
            );
        });
    }

    #[test]
    fn sections_order_groups_by_first_appearance_scratch_last() {
        let keys = vec![
            Some(p("/w/beta")),
            None,
            Some(p("/w/alpha")),
            Some(p("/w/beta")),
        ];
        let sections = sidebar_sections(&keys, &[], &|_| true);
        let shape: Vec<(Option<PathBuf>, Option<String>, Vec<usize>, bool)> = sections
            .into_iter()
            .map(|s| (s.key, s.name, s.tabs, s.pinned))
            .collect();
        assert_eq!(
            shape,
            vec![
                (Some(p("/w/beta")), Some("beta".into()), vec![0, 3], false),
                (Some(p("/w/alpha")), Some("alpha".into()), vec![2], false),
                (None, Some("Scratch".into()), vec![1], false),
            ]
        );

        let flat = sidebar_sections(&[None, None], &[], &|_| true);
        assert_eq!(flat.len(), 1);
        assert_eq!(flat[0].name, None);
        assert_eq!(flat[0].tabs, vec![0, 1]);
        assert!(!flat[0].pinned);
    }

    #[test]
    fn sections_place_pinned_groups_first_in_stored_order_and_keep_empty_ones_on_local() {
        let keys = vec![
            Some(p("/w/beta")),
            None,
            Some(p("/w/alpha")),
            Some(p("/w/gamma")),
        ];
        let pinned = vec![
            "/w/gamma".to_string(),
            "".to_string(),
            "/w/absent".to_string(),
            "/w/alpha".to_string(),
            "/w/gamma".to_string(),
        ];

        let local_sections = sidebar_sections(&keys, &pinned, &|_| true);
        let local_shape: Vec<(Option<PathBuf>, Option<String>, Vec<usize>, bool)> = local_sections
            .into_iter()
            .map(|s| (s.key, s.name, s.tabs, s.pinned))
            .collect();
        assert_eq!(
            local_shape,
            vec![
                (Some(p("/w/gamma")), Some("gamma".into()), vec![3], true),
                (Some(p("/w/absent")), Some("absent".into()), vec![], true),
                (Some(p("/w/alpha")), Some("alpha".into()), vec![2], true),
                (Some(p("/w/beta")), Some("beta".into()), vec![0], false),
                (None, Some("Scratch".into()), vec![1], false),
            ]
        );

        let remote_sections = sidebar_sections(&keys, &pinned, &|_| false);
        let remote_shape: Vec<(Option<PathBuf>, Option<String>, Vec<usize>, bool)> =
            remote_sections
                .into_iter()
                .map(|s| (s.key, s.name, s.tabs, s.pinned))
                .collect();
        assert_eq!(
            remote_shape,
            vec![
                (Some(p("/w/gamma")), Some("gamma".into()), vec![3], true),
                (Some(p("/w/alpha")), Some("alpha".into()), vec![2], true),
                (Some(p("/w/beta")), Some("beta".into()), vec![0], false),
                (None, Some("Scratch".into()), vec![1], false),
            ],
            "absent pinned groups are omitted when keep_empty rejects them"
        );

        let remote_flat = sidebar_sections(&[None, None], &["/w/absent".into()], &|_| false);
        assert_eq!(remote_flat.len(), 1);
        assert_eq!(remote_flat[0].name, None);
        assert_eq!(remote_flat[0].tabs, vec![0, 1]);
    }

    #[test]
    fn reordered_pinned_moves_within_pinned_list() {
        let pinned = vec![
            "/w/alpha".to_string(),
            "/w/beta".to_string(),
            "/w/gamma".to_string(),
        ];
        assert_eq!(
            reordered_pinned(&pinned, &p("/w/gamma"), &p("/w/alpha")),
            Some(vec![
                "/w/gamma".to_string(),
                "/w/alpha".to_string(),
                "/w/beta".to_string(),
            ])
        );
        assert_eq!(
            reordered_pinned(&pinned, &p("/w/alpha"), &p("/w/alpha")),
            None
        );
        assert_eq!(
            reordered_pinned(&pinned, &p("/w/unknown"), &p("/w/alpha")),
            None
        );
    }

    /// The badge on a row and the tab ⌘N opens are two readings of one order,
    /// taken in two places. Grouping makes them diverge from `self.tabs`
    /// order — tab 3 sits in the second row here — so if they are ever read
    /// off different things, the badge names a chord that opens another tab.
    #[test]
    fn a_row_badge_names_the_chord_that_opens_that_row() {
        let keys = vec![
            Some(p("/w/beta")),
            None,
            Some(p("/w/alpha")),
            Some(p("/w/beta")),
        ];
        let pinned = vec!["/w/alpha".to_string()];
        // What `visual_tab_order` returns for a left tab bar.
        let order: Vec<usize> = sidebar_sections(&keys, &pinned, &|_| true)
            .into_iter()
            .flat_map(|s| s.tabs)
            .collect();
        assert_eq!(order, vec![2, 0, 3, 1]);

        let mut badge_pos = vec![0usize; keys.len()];
        for (n, i) in order.iter().copied().enumerate() {
            badge_pos[i] = n;
        }
        for (row, tab) in order.iter().copied().enumerate() {
            // ActivateTabN → activate_visual(N - 1) → order[N - 1].
            let chord = tab_badge_label(badge_pos[tab]);
            let opens = order[badge_pos[tab]];
            assert_eq!(
                opens, tab,
                "row {row} badges ⌘{chord}, which opens tab {opens}"
            );
        }
    }

    #[test]
    fn reordered_rows_moves_within_the_group_only() {
        let keys = vec![
            Some(p("/w/alpha")),
            Some(p("/w/beta")),
            Some(p("/w/alpha")),
            None,
        ];
        let alpha = Some(p("/w/alpha"));
        assert_eq!(
            reordered_rows(&keys, &[], &|_| true, &alpha, &[0, 2], 0, 1),
            Some(vec![2, 0, 1, 3])
        );
        assert_eq!(
            reordered_rows(&keys, &[], &|_| true, &alpha, &[0, 2], 1, 0),
            Some(vec![2, 0, 1, 3])
        );
        assert_eq!(
            reordered_rows(&keys, &[], &|_| true, &alpha, &[0, 2], 1, 1),
            None
        );
    }

    #[test]
    fn reordered_rows_leaves_filtered_out_rows_alone() {
        let keys = vec![Some(p("/w/a")), Some(p("/w/a")), Some(p("/w/a"))];
        let a = Some(p("/w/a"));
        assert_eq!(
            reordered_rows(&keys, &[], &|_| true, &a, &[0, 2], 0, 1),
            Some(vec![1, 2, 0])
        );
    }

    #[test]
    fn regrouped_order_moves_the_group_into_the_target_slot() {
        let keys = vec![
            Some(p("/w/alpha")),
            None,
            Some(p("/w/beta")),
            Some(p("/w/alpha")),
            Some(p("/w/gamma")),
        ];
        assert_eq!(
            regrouped_order(&keys, &p("/w/gamma"), &p("/w/alpha")),
            Some(vec![4, 0, 3, 2, 1])
        );
        assert_eq!(
            regrouped_order(&keys, &p("/w/alpha"), &p("/w/gamma")),
            Some(vec![2, 4, 0, 3, 1])
        );
    }

    #[test]
    fn regrouped_order_ignores_self_and_unknown_roots() {
        let keys = vec![Some(p("/w/alpha")), Some(p("/w/beta"))];
        assert_eq!(regrouped_order(&keys, &p("/w/alpha"), &p("/w/alpha")), None);
        assert_eq!(regrouped_order(&keys, &p("/w/gone"), &p("/w/beta")), None);
        assert_eq!(regrouped_order(&keys, &p("/w/alpha"), &p("/w/gone")), None);
    }

    #[test]
    fn group_names_disambiguate_only_the_collisions() {
        let (a, b, c) = (
            p("/home/u/work/app"),
            p("/home/u/fork/app"),
            p("/home/u/tty7"),
        );
        let names = group_names(&[&a, &b, &c]);
        assert_eq!(names, vec!["work/app", "fork/app", "tty7"]);
    }

    #[test]
    fn group_names_handle_suffix_roots() {
        let (short, long) = (p("/app"), p("/x/app"));
        let names = group_names(&[&short, &long]);
        assert_eq!(names, vec!["app", "x/app"]);
    }
}
