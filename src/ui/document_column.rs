//! The slot the code panel and the diff overlay are drawn in.
//!
//! Both surfaces used to be full-workspace overlays — `absolute`, `inset_0`,
//! `occlude` — so opening a file to read it hid the agent that told you to read
//! it, and reviewing one turned into a toggle loop. They now dock as a column
//! beside the terminal instead, on the same pattern the right panel has always
//! used: a flex sibling with a drag handle and a persisted share of the width.
//!
//! A sibling column, rather than a narrower overlay, is the whole point: the
//! terminal element's laid-out bounds are what drive `set_grid_size`, so a
//! column takes width *away* from the grid and the PTY reflows to what is left.
//! An overlay painted over half the workspace would leave the grid full width
//! with half of it under a card.
//!
//! The overlay is not gone — [`DocumentLayout::Fill`] is exactly the old paint,
//! one command away, and a window too narrow to seat both a terminal and a
//! document falls back to it for that frame without touching what the user
//! saved.

use gpui::{AnyElement, Context, Window, div, prelude::*, px};
use gpui_component::button::Button;
use gpui_component::menu::{PopupMenu, PopupMenuItem};
use gpui_component::{ActiveTheme as _, Icon, IconName, InteractiveElementExt as _, v_flex};
use std::cell::Cell as StdCell;
use std::rc::Rc;

use crate::core::config::{
    Config, DOCUMENT_RATIO_MAX, DOCUMENT_RATIO_MIN, DOCUMENT_RATIO_STOPS, DocumentLayout,
};
use crate::ui::app::{DOCUMENT_MIN_W, OverlayTop, TERMINAL_MIN_W, Tty7App, document_column_px};
use crate::ui::i18n::{L10nKey, t};
use crate::ui::right_panel::RESIZE_HANDLE_WIDTH;

/// Which wrapper a document surface is being asked to paint itself in.
///
/// The *content* of the code panel and of the diff overlay is the same in all
/// three; only the box around it changes, and with it where the header sits and
/// what its gestures mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentChrome {
    /// The historical full-workspace overlay.
    Fill,
    /// A column beside the terminal, carrying its own header as its first row.
    /// macOS only: there the title bar lives inside the terminal column, so the
    /// document column runs to the top of the window and its header lands in
    /// the title strip by itself.
    Dock,
    /// A column beside the terminal whose header has been lifted into the
    /// spanning title bar above it.
    ///
    /// Windows and Linux put the window controls at the right end of a title
    /// bar that spans the workspace, which leaves the strip directly above the
    /// document column empty — a title bar's height of nothing, with the file
    /// name one row below it. The header goes there instead, and the column
    /// renders its body alone.
    DockHoisted,
}

impl DocumentChrome {
    /// Whether this is one of the two column layouts.
    pub(crate) fn is_dock(self) -> bool {
        !matches!(self, DocumentChrome::Fill)
    }

    /// Whether the surface draws its own header, or has had it lifted away.
    pub(crate) fn renders_own_header(self) -> bool {
        !matches!(self, DocumentChrome::DockHoisted)
    }

    /// Whether the header is the strip along the top of the window, and so has
    /// to behave like a title bar — dragging the window, zooming on a
    /// double-click.
    ///
    /// True for the overlay, whose header stands in for the title bar. True for
    /// a hoisted header, which is drawn *into* the title bar. True for a docked
    /// column only on macOS, where the column reaches the top of the window
    /// anyway. False for a plain docked header, which sits inside the workspace
    /// and would be a second, fake title bar if it moved the window.
    pub(crate) fn header_is_title_strip(self) -> bool {
        match self {
            DocumentChrome::Fill | DocumentChrome::DockHoisted => true,
            DocumentChrome::Dock => cfg!(target_os = "macos"),
        }
    }
}

impl Tty7App {
    /// The layout the active tab's document was asked for, which is not always
    /// the one it gets — see [`Tty7App::document_dock_px`].
    ///
    /// A tab that has never been told follows the config, which holds the last
    /// explicit choice anyone made and is therefore what a fresh tab starts
    /// from. Telling one tab never moves another.
    pub(crate) fn document_layout(&self, cx: &gpui::App) -> DocumentLayout {
        self.tabs
            .get(self.active)
            .and_then(|t| t.document_layout)
            .unwrap_or_else(|| cx.global::<Config>().document_layout)
    }

    /// Which of the two surfaces the docked column shows.
    ///
    /// `overlay_top` orders a *pair* of overlays in fill mode, where both are
    /// painted and the front one wins on paint order. A column has one child,
    /// so the ordering has to become a choice: the surface on top, unless it is
    /// closed, in which case there is no front and the survivor is it.
    pub(crate) fn document_front(&self) -> Option<OverlayTop> {
        let tab = self.tabs.get(self.active)?;
        let code = tab.code.as_ref().is_some_and(|c| c.visible);
        let diff = tab.diff_overlay.is_some();
        match (tab.overlay_top, code, diff) {
            (_, false, false) => None,
            (OverlayTop::Code, true, _) | (OverlayTop::Diff, true, false) => Some(OverlayTop::Code),
            (OverlayTop::Diff, _, true) | (OverlayTop::Code, false, true) => Some(OverlayTop::Diff),
        }
    }

    /// What the document column has reserved, from a side panel's point of
    /// view.
    ///
    /// Read from the user's intent and from whether a surface is open — never
    /// from the *effective* layout, which is derived from the widths this feeds
    /// and would close the loop on itself.
    pub(crate) fn document_floor(&self, cx: &gpui::App) -> f32 {
        if self.document_layout(cx) == DocumentLayout::Dock && self.document_front().is_some() {
            DOCUMENT_MIN_W
        } else {
            0.
        }
    }

    /// The width the terminal and a docked document share: the window less
    /// whichever side panels are open, at the widths they are actually drawn
    /// at rather than at their floors. A sidebar someone dragged wider is width
    /// the terminal no longer has.
    pub(crate) fn document_body_px(&self, window: &Window, cx: &gpui::App) -> f32 {
        let viewport = window.viewport_size().width.as_f32();
        let sidebar = if self.sidebar_open(cx) {
            self.sidebar_px(window, cx)
        } else {
            0.
        };
        let panel = if self.right_panel_open(cx) {
            self.right_panel_px(window, cx)
        } else {
            0.
        };
        viewport - sidebar - panel
    }

    /// How wide the document column is drawn this frame, or `None` when the
    /// surface is closed, the user chose fill, or the window is too narrow to
    /// seat both.
    pub(crate) fn document_dock_px(&self, window: &Window, cx: &gpui::App) -> Option<f32> {
        if self.document_layout(cx) != DocumentLayout::Dock || self.document_front().is_none() {
            return None;
        }
        document_column_px(self.document_body_px(window, cx), self.document_ratio.get())
    }

    /// macOS's docked title bar belongs to the terminal column, so its chrome
    /// would move by the document's width when filling. Give both layouts the
    /// same window corner; other platforms already span the docked title bar.
    pub(crate) fn document_chrome_in_corner(&self, window: &Window, cx: &gpui::App) -> bool {
        self.document_front().is_some()
            && !self.right_panel_open(cx)
            && (cfg!(target_os = "macos") || self.document_dock_px(window, cx).is_none())
    }

    /// With a narrow workspace, the document actions and window controls
    /// cannot share one row. Keep the document header below the window chrome
    /// rather than hide its controls or collapse the session sidebar.
    pub(crate) fn document_header_below_chrome(&self, window: &Window, cx: &gpui::App) -> bool {
        let actions = 3. * crate::ui::app::TILE_SIZE + 6. * crate::ui::app::CONTENT_INSET;
        !self.right_panel_open(cx)
            && self.document_body_px(window, cx)
                < crate::ui::tab_strip::trailing_chrome_w() + actions
    }

    /// Fill ↔ dock, for the active tab. One of the two writers of the layout;
    /// the narrow window fallback is not, on purpose — running this while the
    /// fallback is showing is the user saying they meant the overlay, and that
    /// is worth keeping.
    pub(crate) fn toggle_document_fill(&mut self, cx: &mut Context<Self>) {
        let next = match self.document_layout(cx) {
            DocumentLayout::Dock => DocumentLayout::Fill,
            DocumentLayout::Fill => DocumentLayout::Dock,
        };
        self.set_document_layout(next, cx);
    }

    /// Snap the column to a named share of the terminal column. Docks first if
    /// the surface is filling the window: asking for a third of the width is
    /// asking for a column.
    pub(crate) fn set_document_ratio(&mut self, ratio: f32, cx: &mut Context<Self>) {
        self.document_ratio.set(ratio);
        self.update_config(cx, |cfg| cfg.document_ratio = ratio);
        self.set_document_layout(DocumentLayout::Dock, cx);
    }

    /// Third → half → two thirds → third, the double-click on the divider.
    /// Starts from whichever named width the current one is nearest, so a
    /// dragged column joins the cycle where it looks like it is.
    pub(crate) fn cycle_document_ratio(&mut self, cx: &mut Context<Self>) {
        let current = self.document_ratio.get();
        let nearest = next_ratio_stop(current);
        self.set_document_ratio(nearest, cx);
    }

    /// Point the active tab at a layout, and only that tab.
    ///
    /// Deliberately not written back to `document_layout` in the config: that
    /// key is the value a tab starts from, and a tab that has not been told is
    /// still reading it. Writing it here would reach every one of those at
    /// once, which is the window-wide switch this is not.
    ///
    /// The narrow-window fallback does not come through here either — it is
    /// derived at render time and stored nowhere.
    pub(crate) fn set_document_layout(&mut self, layout: DocumentLayout, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.document_layout = Some(layout);
        }
        cx.notify();
    }

    /// A shared, visible layout action for code and diff headers. The saved
    /// preference drives it; a temporary narrow-window fallback is not a new
    /// preference and must not change what another tab or a wider window does.
    pub(crate) fn document_fill_button(&self, cx: &mut Context<Self>) -> AnyElement {
        // Root clears reading selections before button mouse-down handlers.
        // Capture during render, then remember that snapshot on the press so
        // a redraw between mouse-down and mouse-up cannot replace it.
        let selection = (self.document_front() == Some(OverlayTop::Code))
            .then(|| self.tab_code()?.active_file()?.reading.clone())
            .flatten()
            .map(|reading| {
                let snapshot = reading.read(cx).text.read(cx).selection_snapshot();
                (reading, snapshot)
            });
        let filled = self.document_layout(cx) == DocumentLayout::Fill;
        let (icon, tooltip) = if filled {
            (IconName::Minimize, L10nKey::DocumentRestoreTooltip)
        } else {
            (IconName::Maximize, L10nKey::DocumentFillTooltip)
        };
        let button = div()
            .debug_selector(|| "document-fill-toggle".into())
            .occlude()
            .flex_shrink_0()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |_, _, _, cx| {
                    if let Some((reading, snapshot)) = &selection {
                        reading.update(cx, |reading, _| {
                            reading.remember_layout_selection(snapshot.clone())
                        });
                    }
                }),
            )
            .child(
                crate::ui::tab_strip::chrome_tile_sized(
                    Button::new("document-fill-toggle").icon(Icon::new(icon)),
                    crate::ui::app::TILE_SIZE,
                    crate::ui::app::TILE_GLYPH_LINE,
                    false,
                    cx,
                )
                .rounded_lg()
                .tooltip(t(tooltip))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_document_fill(cx);
                    if this.document_front() == Some(OverlayTop::Code)
                        && let Some(reading) = this
                            .tab_code()
                            .and_then(|code| code.active_file())
                            .and_then(|file| file.reading.clone())
                    {
                        reading.update(cx, |reading, cx| reading.restore_layout_selection(cx));
                    }
                })),
            );
        #[cfg(feature = "markdown-visual-tests")]
        let button = button.relative().child(
            gpui::canvas(
                |bounds, _, cx| crate::document_visual::record_fill_target(bounds, cx),
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
            .size_full(),
        );
        button.into_any_element()
    }

    /// Explicit layout choices remain available alongside the visible toggle.
    pub(crate) fn document_header_menu(
        menu: PopupMenu,
        app: &gpui::WeakEntity<Self>,
        cx: &gpui::App,
    ) -> PopupMenu {
        let docked = app
            .upgrade()
            .is_none_or(|this| this.read(cx).document_layout(cx) == DocumentLayout::Dock);
        let mut menu = menu.min_w(px(220.));

        for (label, layout) in [
            (L10nKey::DocumentDock, DocumentLayout::Dock),
            (L10nKey::DocumentFill, DocumentLayout::Fill),
        ] {
            menu = menu.item(
                PopupMenuItem::new(t(label))
                    .checked(docked == (layout == DocumentLayout::Dock))
                    .on_click({
                        let app = app.clone();
                        move |_, _window, cx| {
                            let _ = app.update(cx, |this, cx| this.set_document_layout(layout, cx));
                        }
                    }),
            );
        }
        menu
    }

    /// The column itself: the surface `overlay_top` selects, sized, bordered,
    /// and given the divider on its left edge.
    /// The header on its own, for the strip above the column — see
    /// [`DocumentChrome::DockHoisted`]. Returns `None` for the layouts that
    /// keep their header inside the surface.
    pub(crate) fn render_document_header(
        &mut self,
        chrome: DocumentChrome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if chrome.renders_own_header() {
            return None;
        }
        match self.document_front()? {
            OverlayTop::Code => Some(self.render_editor_header_only(chrome, window, cx)),
            OverlayTop::Diff => self.render_diff_header_only(chrome, window, cx),
        }
    }

    pub(crate) fn render_document_column(
        &mut self,
        width: f32,
        chrome: DocumentChrome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let body = self.document_body_px(window, cx);
        let surface = match self.document_front()? {
            OverlayTop::Code => self.render_code_overlay(chrome, window, cx),
            OverlayTop::Diff => self.render_diff_overlay(chrome, window, cx),
        }?;
        let (backing, handle) = self.document_resize(body, cx);
        Some(
            v_flex()
                .id("document-column")
                .relative()
                .flex_none()
                .w(px(width))
                .h_full()
                .bg(crate::ui::theme::workspace_surface_color(cx))
                .border_l_1()
                .border_color(cx.theme().sidebar_border)
                .child(backing)
                // The clip belongs to the content, not to the column: the
                // divider hangs half a handle past the left edge, the way the
                // panels' do, and clipping the column would have taken that
                // half — and the grab with it — away.
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .overflow_hidden()
                        .child(surface),
                )
                .child(handle)
                .into_any_element(),
        )
    }

    /// The divider, on the same contract as the right panel's: the cell moves
    /// on every mouse move, the config is written once, on mouse up.
    ///
    /// `body` is read here, while there is still a `cx` to read it from — the
    /// drag handler only ever sees a `Window`, and a limit that disagreed with
    /// the one the layout applies would spring the column back from wherever it
    /// was dropped.
    fn document_resize(&self, body: f32, cx: &mut Context<Self>) -> (AnyElement, AnyElement) {
        use gpui::{Bounds, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels, canvas};

        let container: Rc<StdCell<Option<Bounds<Pixels>>>> = Rc::new(StdCell::new(None));
        let backing = canvas(
            {
                let container = container.clone();
                move |bounds, _window, _cx| container.set(Some(bounds))
            },
            {
                let container = container.clone();
                let ratio_cell = self.document_ratio.clone();
                let dragging = self.document_dragging.clone();
                move |_bounds, _state, window, _cx| {
                    window.on_mouse_event({
                        let container = container.clone();
                        let ratio_cell = ratio_cell.clone();
                        let dragging = dragging.clone();
                        move |ev: &MouseMoveEvent, _phase, window, _cx| {
                            if !dragging.get() || body <= 0. {
                                return;
                            }
                            let Some(b) = container.get() else {
                                return;
                            };
                            let right = b.origin.x + b.size.width;
                            let raw = (right - ev.position.x).as_f32();
                            ratio_cell.set(dragged_ratio(body, raw));
                            window.refresh();
                        }
                    });
                    window.on_mouse_event({
                        let ratio_cell = ratio_cell.clone();
                        let dragging = dragging.clone();
                        move |_ev: &MouseUpEvent, _phase, window, cx| {
                            if !dragging.get() {
                                return;
                            }
                            dragging.set(false);
                            let r = ratio_cell.get();
                            let cfg = cx.global_mut::<Config>();
                            if cfg.document_ratio != r {
                                cfg.document_ratio = r;
                                cfg.save();
                            }
                            window.refresh();
                        }
                    });
                }
            },
        )
        .absolute()
        .size_full()
        .into_any_element();

        let active = self.document_dragging.get();
        let handle = div()
            .id("document-resize")
            .group("document-resize")
            .occlude()
            .absolute()
            .top_0()
            .left(px(-(RESIZE_HANDLE_WIDTH / 2.)))
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
                    .when(active, |d| d.bg(cx.theme().drag_border))
                    .group_hover("document-resize", |s| s.bg(cx.theme().drag_border)),
            )
            .on_mouse_down(MouseButton::Left, {
                let dragging = self.document_dragging.clone();
                move |_ev, window, _cx| {
                    dragging.set(true);
                    window.refresh();
                }
            })
            // A double-click lands a mouse-down first, which arms the drag; the
            // mouse-up disarms it without having moved, so the cycle below is
            // the only thing that ends up happening.
            .on_double_click(cx.listener(|this, _, _window, cx| {
                this.document_dragging.set(false);
                this.cycle_document_ratio(cx);
            }))
            .into_any_element();

        (backing, handle)
    }
}

/// The ratio a divider dropped `raw` points from the right edge of a `body`
/// wide area settles on.
///
/// Two clamps, and both are load-bearing. The pixel one keeps the terminal and
/// the document each above their floor, and is what binds on a narrow window.
/// The ratio one is the band the *file* keeps — `Config::sanitize` holds
/// `document_ratio` to it, so a drag that wrote outside it would be moved on
/// the next launch, and a column dropped against the edge of a wide window
/// would reopen hundreds of points from where it was left.
pub(crate) fn dragged_ratio(body: f32, raw: f32) -> f32 {
    let w = raw.clamp(DOCUMENT_MIN_W, (body - TERMINAL_MIN_W).max(DOCUMENT_MIN_W));
    (w / body).clamp(DOCUMENT_RATIO_MIN, DOCUMENT_RATIO_MAX)
}

/// The width the divider's double-click moves to from `current`: the one after
/// whichever named stop `current` is nearest, wrapping round.
pub(crate) fn next_ratio_stop(current: f32) -> f32 {
    let nearest = DOCUMENT_RATIO_STOPS
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - current).abs().total_cmp(&(*b - current).abs()))
        .map(|(i, _)| i)
        .unwrap_or(1);
    DOCUMENT_RATIO_STOPS[(nearest + 1) % DOCUMENT_RATIO_STOPS.len()]
}

#[cfg(test)]
mod tests {
    use super::{dragged_ratio, next_ratio_stop};
    use crate::core::config::{
        DOCUMENT_RATIO_HALF, DOCUMENT_RATIO_MAX, DOCUMENT_RATIO_MIN, DOCUMENT_RATIO_THIRD,
        DOCUMENT_RATIO_TWO_THIRDS,
    };
    use crate::ui::app::{DOCUMENT_MIN_W, TERMINAL_MIN_W, document_column_px};

    /// Nothing the divider can write is something the next launch moves.
    /// `Config::sanitize` clamps `document_ratio` into a band, so a drag that
    /// left the band was saved and then quietly relocated: on a 2560-point body
    /// the narrowest the divider went was a ratio of 0.11, which came back as
    /// 0.2 — a 232-point jump the user never asked for. The drag clamps to the
    /// file's band as well as to the two floors now, and this is what says so.
    #[test]
    fn a_dragged_width_is_one_the_config_can_keep() {
        for body in [640., 900., 1440., 2560., 5120.] {
            for raw in [-500., 0., 1., DOCUMENT_MIN_W, body / 2., body, body + 900.] {
                let r = dragged_ratio(body, raw);
                assert!(
                    (DOCUMENT_RATIO_MIN..=DOCUMENT_RATIO_MAX).contains(&r),
                    "body {body} dropped at {raw} wrote {r}, which the file would not keep"
                );
                // And the width that ratio draws still holds the invariant the
                // whole budget exists for.
                let drawn = document_column_px(body, r).expect("a body that seats both");
                assert!(body - drawn >= TERMINAL_MIN_W - f32::EPSILON);
                assert!(drawn >= DOCUMENT_MIN_W - f32::EPSILON);
            }
        }
    }

    /// Third, half, two thirds, round again — and a dragged width joins at
    /// whichever stop it looks nearest to rather than always restarting.
    #[test]
    fn the_divider_cycles_the_named_widths() {
        assert_eq!(next_ratio_stop(DOCUMENT_RATIO_THIRD), DOCUMENT_RATIO_HALF);
        assert_eq!(
            next_ratio_stop(DOCUMENT_RATIO_HALF),
            DOCUMENT_RATIO_TWO_THIRDS
        );
        assert_eq!(
            next_ratio_stop(DOCUMENT_RATIO_TWO_THIRDS),
            DOCUMENT_RATIO_THIRD
        );
        // Dragged to just under half: nearest stop is half, so the cycle goes
        // on to two thirds rather than back to a third.
        assert_eq!(next_ratio_stop(0.47), DOCUMENT_RATIO_TWO_THIRDS);
        assert_eq!(next_ratio_stop(0.8), DOCUMENT_RATIO_THIRD);
    }

    /// The default is half of what the terminal and the document share, and
    /// what they share is the window less the panels — not the window.
    #[test]
    fn half_of_the_body_is_half_of_the_body() {
        let body = 1440. - 220. - 260.;
        assert_eq!(document_column_px(body, 0.5), Some(body / 2.));
    }

    /// Two thirds is allowed past the half-window cap the side panels obey.
    /// Only the terminal's floor binds it.
    #[test]
    fn two_thirds_is_two_thirds_until_the_terminal_floor_says_otherwise() {
        let wide = 1200.;
        assert_eq!(document_column_px(wide, 2. / 3.), Some(wide * 2. / 3.));

        // 800 * 2/3 is 533, which would leave the terminal 267 — under its
        // floor — so the column stops where the terminal starts.
        let tight = 800.;
        assert_eq!(
            document_column_px(tight, 2. / 3.),
            Some(tight - TERMINAL_MIN_W)
        );
    }

    /// A column narrower than it can be read at is not a column. The ratio
    /// floors out rather than shrinking with the window.
    #[test]
    fn a_thin_share_still_gets_the_documents_floor() {
        let body = 700.;
        assert_eq!(document_column_px(body, 0.2), Some(DOCUMENT_MIN_W));
    }

    /// Below the width where both fit there is no docked layout to draw, and
    /// the caller falls back to the overlay for the frame. The threshold is
    /// exact so that widening by a point re-docks.
    #[test]
    fn a_window_too_narrow_for_both_has_no_docked_width() {
        let floor = TERMINAL_MIN_W + DOCUMENT_MIN_W;
        assert_eq!(document_column_px(floor - 1., 0.5), None);
        assert_eq!(document_column_px(floor, 0.5), Some(DOCUMENT_MIN_W));
        assert_eq!(document_column_px(f32::NAN, 0.5), None);
    }

    /// Whatever the terminal is left, it is never less than its floor. That is
    /// the invariant the whole budget exists for.
    #[test]
    fn the_terminal_keeps_its_floor_at_every_share() {
        for body in [640., 700., 900., 1200., 2400.] {
            for ratio in [0.2, 1. / 3., 0.5, 2. / 3., 0.8] {
                let Some(doc) = document_column_px(body, ratio) else {
                    continue;
                };
                assert!(
                    body - doc >= TERMINAL_MIN_W - f32::EPSILON,
                    "body {body} ratio {ratio} left the terminal {}",
                    body - doc
                );
                assert!(doc >= DOCUMENT_MIN_W - f32::EPSILON);
            }
        }
    }
}

#[cfg(test)]
mod gpui_tests {
    use super::*;
    use crate::core::config::{DOCUMENT_RATIO_TWO_THIRDS, DocumentLayout};
    use crate::ui::app::test_window;
    use crate::ui::pane::{Pane, PaneSlot};
    use crate::ui::pending_pane::{PendingPane, PendingSpawn};
    use gpui::{Entity, TestAppContext, VisualTestContext, px, size};

    /// One quiet tab. The pane is a *connecting* one so the harness needs no
    /// PTY and runs on every platform.
    fn push_tab(app: &mut Tty7App, cx: &mut Context<Tty7App>) {
        let pending = cx.new(|cx| {
            PendingPane::new(
                "test-box",
                PendingSpawn {
                    workspace: None,
                    working_directory: None,
                    restore_pane: None,
                    shell: None,
                    agent: None,
                    agent_session_id: None,
                    agent_launch_argv: None,
                    agent_restore_pending: false,
                    agent_unstarted: false,
                    owner: None,
                    font_size: 14.0,
                },
                cx,
            )
        });
        app.tabs
            .push(crate::ui::app::Tab::new(Pane::leaf(PaneSlot::Connecting(
                pending,
            ))));
        cx.notify();
    }

    /// A window with `tabs` quiet tabs, active on the first, sized to order.
    fn window_with(
        cx: &mut TestAppContext,
        w: f32,
        tabs: usize,
    ) -> (Entity<Tty7App>, VisualTestContext) {
        let (app, mut vcx) = test_window::harness(cx);
        app.update_in(&mut vcx, |app, _, cx| {
            for _ in 0..tabs {
                push_tab(app, cx);
            }
            app.active = 0;
        });
        vcx.simulate_resize(size(px(w), px(900.)));
        vcx.run_until_parked();
        (app, vcx)
    }

    fn window(cx: &mut TestAppContext, w: f32) -> (Entity<Tty7App>, VisualTestContext) {
        window_with(cx, w, 1)
    }

    fn dock_px(app: &Entity<Tty7App>, vcx: &mut VisualTestContext) -> Option<f32> {
        app.update_in(vcx, |app, window, cx| app.document_dock_px(window, cx))
    }

    /// The config value: the default under tabs that were never told, not
    /// necessarily what the active tab is doing.
    fn layout(vcx: &mut VisualTestContext) -> DocumentLayout {
        vcx.update(|_, cx| cx.global::<Config>().document_layout)
    }

    /// What the active tab is actually doing.
    fn tab_layout(app: &Entity<Tty7App>, vcx: &mut VisualTestContext) -> DocumentLayout {
        app.update_in(vcx, |app, _, cx| app.document_layout(cx))
    }

    #[gpui::test]
    fn diff_character_selection_works_in_the_docked_and_full_window(cx: &mut TestAppContext) {
        use crate::core::config::DiffViewMode;
        use crate::terminal::git_diff::{self, DiffSnapshot, DiffSource};
        use crate::ui::diff_overlay::{DiffLoad, DiffOverlayState};
        use crate::ui::host_ops::HostId;
        use gpui::{Modifiers, MouseButton, ScrollHandle, point};
        use std::{collections::HashMap, path::PathBuf, sync::Arc};

        let (app, mut vcx) = window(cx, 1600.);
        let source = DiffSource::commit("selection-fixture");
        let snapshot = Arc::new(DiffSnapshot {
            source: source.clone(),
            files: vec![git_diff::synthesize_added(
                "a.txt",
                b"before selected after\nsecond line\n",
                &git_diff::DiffBudget::SINGLE_FILE,
            )],
            ..Default::default()
        });
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            for layout in [DocumentLayout::Dock, DocumentLayout::Fill] {
                app.update_in(&mut vcx, |app, window, cx| {
                    cx.global_mut::<Config>().diff_view = mode;
                    app.set_document_layout(layout, cx);
                    let focus_handle = cx.focus_handle();
                    window.focus(&focus_handle, cx);
                    let active = app.active;
                    app.tabs[active].overlay_top = OverlayTop::Diff;
                    app.tabs[active].diff_overlay = Some(DiffOverlayState {
                        pane_host: None,
                        host_id: HostId::LOCAL,
                        cwd: PathBuf::from("/no/such/repo"),
                        source: source.clone(),
                        commit: None,
                        commit_message_expanded: false,
                        commit_split: crate::ui::diff_overlay::CommitSplit::new(cx),
                        document_header_focus: cx.focus_handle(),
                        focus_handle,
                        load: DiffLoad::Ready(snapshot.clone()),
                        loading: false,
                        expanded: HashMap::new(),
                        focus: Some("a.txt".into()),
                        preview: None,
                        preview_loading: None,
                        scroll: ScrollHandle::new(),
                        epoch: None,
                        file_views: Default::default(),
                        geometry: Default::default(),
                        anchor: Default::default(),
                    });
                    cx.notify();
                });
                vcx.run_until_parked();
                let column = usize::from(mode == DiffViewMode::Split);
                let bounds = vcx
                    .debug_bounds(if column == 0 {
                        "diff-selection-text-0-0"
                    } else {
                        "diff-selection-text-1-0"
                    })
                    .unwrap();
                let prefix = if mode == DiffViewMode::Split {
                    "+ "
                } else {
                    ""
                };
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
                vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
                vcx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
                vcx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
                vcx.run_until_parked();
                vcx.simulate_keystrokes("secondary-c");
                assert_eq!(
                    vcx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
                    "selected"
                );

                let word = point((start.x + end.x) / 2., start.y);
                for (count, expected) in [(2, "selected"), (3, "before selected after")] {
                    vcx.simulate_event(gpui::MouseDownEvent {
                        button: MouseButton::Left,
                        position: word,
                        click_count: count,
                        ..Default::default()
                    });
                    vcx.run_until_parked();
                    vcx.simulate_mouse_up(word, MouseButton::Left, Modifiers::none());
                    vcx.simulate_keystrokes("secondary-c");
                    assert_eq!(
                        vcx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
                        expected
                    );
                }
                vcx.simulate_click(start, Modifiers::none());
                vcx.simulate_click(
                    end,
                    Modifiers {
                        shift: true,
                        ..Modifiers::none()
                    },
                );
                vcx.simulate_keystrokes("secondary-c");
                assert_eq!(
                    vcx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
                    "selected"
                );

                vcx.update(|_, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string("UNCHANGED".into()))
                });
                vcx.simulate_mouse_down(start, MouseButton::Right, Modifiers::none());
                vcx.simulate_mouse_up(start, MouseButton::Right, Modifiers::none());
                vcx.run_until_parked();
                vcx.simulate_keystrokes("down enter");
                vcx.run_until_parked();
                assert_eq!(
                    vcx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
                    "selected",
                    "context menu in {layout:?} {mode:?}"
                );

                vcx.simulate_keystrokes("secondary-a secondary-c");
                assert_eq!(
                    vcx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
                    "before selected after\nsecond line"
                );
                vcx.update(|_, cx| {
                    cx.global_mut::<Config>().diff_view = if mode == DiffViewMode::Split {
                        DiffViewMode::Unified
                    } else {
                        DiffViewMode::Split
                    };
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string("UNCHANGED".into()));
                });
                app.update_in(&mut vcx, |_, _, cx| cx.notify());
                vcx.run_until_parked();
                vcx.simulate_keystrokes("secondary-c");
                assert_eq!(
                    vcx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
                    "UNCHANGED",
                    "mode change must discard stale ranges"
                );

                let content = format!("before selected after\n{}", "more code\n".repeat(120));
                app.update_in(&mut vcx, |app, _, cx| {
                    cx.global_mut::<Config>().diff_view = mode;
                    let active = app.active;
                    app.tabs[active].diff_overlay.as_mut().unwrap().load =
                        DiffLoad::Ready(Arc::new(DiffSnapshot {
                            source: source.clone(),
                            files: vec![git_diff::synthesize_added(
                                "a.txt",
                                content.as_bytes(),
                                &git_diff::DiffBudget::SINGLE_FILE,
                            )],
                            ..Default::default()
                        }));
                    cx.notify();
                });
                vcx.run_until_parked();
                let viewport = vcx.debug_bounds("diff-scroll").unwrap();
                let outside = point(start.x, viewport.bottom() + px(30.));
                vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
                vcx.simulate_mouse_move(outside, MouseButton::Left, Modifiers::none());
                vcx.run_until_parked();
                vcx.executor()
                    .advance_clock(std::time::Duration::from_millis(32));
                vcx.run_until_parked();
                let offset = app.read_with(&vcx, |app, _| {
                    app.tabs[app.active]
                        .diff_overlay
                        .as_ref()
                        .unwrap()
                        .scroll
                        .offset()
                });
                assert!(
                    offset.y < px(0.),
                    "selection must scroll below the viewport"
                );
                vcx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
                vcx.executor()
                    .advance_clock(std::time::Duration::from_millis(64));
                vcx.run_until_parked();
                assert_eq!(
                    app.read_with(&vcx, |app, _| app.tabs[app.active]
                        .diff_overlay
                        .as_ref()
                        .unwrap()
                        .scroll
                        .offset()),
                    offset,
                    "release outside the code must stop scrolling"
                );
                vcx.simulate_keystrokes("secondary-c");
                assert!(
                    vcx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap())
                        .contains("selected after\nmore code")
                );
            }
        }
    }

    #[gpui::test]
    fn github_diff_mode_switch_keeps_the_open_patch_and_reading_position(cx: &mut TestAppContext) {
        use crate::core::config::DiffViewMode;
        use crate::terminal::git_diff::{self, DiffSnapshot, DiffSource};
        use crate::ui::diff_overlay::{DiffLoad, DiffOverlayState};
        use crate::ui::host_ops::HostId;
        use gpui::{ScrollHandle, point};
        use gpui_component::{Theme, ThemeMode};
        use std::{collections::HashMap, path::PathBuf, sync::Arc};

        let (app, mut vcx) = window(cx, 1600.);
        let tracked = git_diff::synthesize_added(
            "a.txt",
            "line\n".repeat(120).as_bytes(),
            &git_diff::DiffBudget::SINGLE_FILE,
        );
        let preview = Arc::new(git_diff::synthesize_added(
            "new.txt",
            "preview\n".repeat(120).as_bytes(),
            &git_diff::DiffBudget::SINGLE_FILE,
        ));
        let source = DiffSource::commit("fixed-test-patch");
        let snapshot = Arc::new(DiffSnapshot {
            source: source.clone(),
            files: vec![tracked],
            untracked: vec!["new.txt".into()],
            untracked_total: 1,
            ..Default::default()
        });
        for mode in [DiffViewMode::Split, DiffViewMode::Unified] {
            for path in ["a.txt", "new.txt"] {
                let scroll = ScrollHandle::new();
                let expanded =
                    HashMap::from([("a.txt".into(), true), ("collapsed.txt".into(), false)]);
                app.update_in(&mut vcx, |app, window, cx| {
                    cx.global_mut::<Config>().diff_view = mode;
                    let focus_handle = cx.focus_handle();
                    window.focus(&focus_handle, cx);
                    let active = app.active;
                    app.tabs[active].overlay_top = OverlayTop::Diff;
                    app.tabs[active].diff_overlay = Some(DiffOverlayState {
                        pane_host: None,
                        host_id: HostId::LOCAL,
                        cwd: PathBuf::from("/no/such/tty7/repo"),
                        source: source.clone(),
                        commit: None,
                        commit_message_expanded: false,
                        commit_split: crate::ui::diff_overlay::CommitSplit::new(cx),
                        document_header_focus: cx.focus_handle(),
                        focus_handle,
                        load: DiffLoad::Ready(Arc::clone(&snapshot)),
                        loading: false,
                        expanded: expanded.clone(),
                        focus: Some(path.into()),
                        preview: Some(("new.txt".into(), Some(Arc::clone(&preview)))),
                        preview_loading: None,
                        scroll: scroll.clone(),
                        epoch: None,
                        file_views: Default::default(),
                        geometry: Default::default(),
                        anchor: Default::default(),
                    });
                    cx.notify();
                });
                vcx.run_until_parked();
                scroll.set_offset(point(px(0.), px(-120.)));
                let before = scroll.offset();
                assert_eq!(before.y, px(-120.), "the fixture must be scrollable");
                for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::Light] {
                    vcx.update(|window, cx| Theme::change(theme, Some(window), cx));
                    vcx.run_until_parked();
                    assert!(vcx.debug_bounds("diff-panel").is_some());
                    app.update_in(&mut vcx, |app, window, _| {
                        let overlay = app.tabs[app.active].diff_overlay.as_ref().unwrap();
                        let DiffLoad::Ready(held) = &overlay.load else {
                            panic!("patch was replaced")
                        };
                        assert!(Arc::ptr_eq(held, &snapshot));
                        assert_eq!(overlay.focus.as_deref(), Some(path));
                        assert_eq!(overlay.expanded, expanded);
                        assert_eq!(overlay.scroll.offset(), before);
                        assert!(overlay.focus_handle.is_focused(window));
                        assert!(Arc::ptr_eq(
                            overlay.preview.as_ref().unwrap().1.as_ref().unwrap(),
                            &preview
                        ));
                    });
                }
            }
        }
    }

    #[gpui::test]
    fn visible_fill_buttons_preserve_sidebars_and_restore_width(cx: &mut TestAppContext) {
        let (app, mut vcx) = window(cx, 1600.);
        vcx.update(|_, cx| {
            cx.global_mut::<Config>().tab_bar_position = crate::core::config::TabBarPosition::Left;
        });
        app.update_in(&mut vcx, |app, _, cx| {
            app.sidebar_collapsed = false;
            app.sidebar_width.set(210.);
            app.right_panel_visible = true;
            app.right_panel_width.set(240.);
            app.right_panel_tab = crate::core::config::RightPanelTab::Files;
            app.set_document_ratio(0.57, cx);
        });
        for front in [OverlayTop::Code, OverlayTop::Diff] {
            app.update_in(&mut vcx, |app, window, cx| match front {
                OverlayTop::Code => app.toggle_code_panel(window, cx),
                OverlayTop::Diff => app.open_diff_overlay(
                    crate::ui::host_ops::HostId::LOCAL,
                    std::path::PathBuf::from("/no/such/tty7/repo"),
                    crate::terminal::git_diff::DiffSource::Head,
                    None,
                    window,
                    cx,
                ),
            });
            for theme in [
                gpui_component::ThemeMode::Light,
                gpui_component::ThemeMode::Dark,
            ] {
                vcx.update(|window, cx| gpui_component::Theme::change(theme, Some(window), cx));
                vcx.run_until_parked();
                let surface = match front {
                    OverlayTop::Code => "code-panel",
                    OverlayTop::Diff => "diff-panel",
                };

                let fill = vcx
                    .debug_bounds("document-fill-toggle")
                    .expect("fill is visible without a menu");
                let before = vcx.debug_bounds(surface).expect("the document is drawn");
                let sidebar = vcx
                    .debug_bounds("session-sidebar")
                    .expect("the session sidebar is drawn");
                let files = vcx
                    .debug_bounds("right-panel")
                    .expect("the file tree is drawn");
                vcx.simulate_click(fill.center(), gpui::Modifiers::none());
                vcx.run_until_parked();

                let filled = vcx
                    .debug_bounds(surface)
                    .expect("the document remains open");
                assert_eq!(vcx.debug_bounds("session-sidebar"), Some(sidebar));
                assert_eq!(vcx.debug_bounds("right-panel"), Some(files));
                assert_eq!(
                    filled.left(),
                    sidebar.right(),
                    "fill starts after the session sidebar"
                );
                assert_eq!(
                    filled.right(),
                    files.left(),
                    "the open file tree stays available"
                );
                assert!(
                    filled.size.width > before.size.width,
                    "code covers the agent area"
                );

                let restore = vcx
                    .debug_bounds("document-fill-toggle")
                    .expect("restore remains visible");
                vcx.simulate_click(restore.center(), gpui::Modifiers::none());
                vcx.run_until_parked();
                assert_eq!(
                    vcx.debug_bounds(surface),
                    Some(before),
                    "restore keeps the chosen split"
                );
            }
            app.update_in(&mut vcx, |app, window, cx| match front {
                OverlayTop::Code => app.toggle_code_panel(window, cx),
                OverlayTop::Diff => app.close_diff_overlay(window, cx),
            });
        }
    }

    #[gpui::test]
    fn diff_cards_and_rows_fill_the_content_width(cx: &mut TestAppContext) {
        use crate::core::config::DiffViewMode;
        use crate::terminal::git_diff::{
            DiffLine, DiffSnapshot, DiffSource, FileDiff, FileStatus, Hunk, LineKind,
        };
        use crate::ui::diff_overlay::DiffLoad;
        use std::sync::Arc;

        let (app, mut vcx) = window(cx, 1600.);
        app.update_in(&mut vcx, |app, window, cx| {
            app.open_diff_overlay(
                crate::ui::host_ops::HostId::LOCAL,
                std::path::PathBuf::from("/no/such/tty7/repo"),
                DiffSource::Worktree,
                None,
                window,
                cx,
            );
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            vcx.run_until_parked();
            if app.update_in(&mut vcx, |app, _, _| {
                !app.tabs[app.active].diff_overlay.as_ref().unwrap().loading
            }) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "diff probe did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let file = |path: &str, text: String, lines: u32| FileDiff {
            path: path.into(),
            old_path: None,
            status: FileStatus::Modified,
            added: lines,
            removed: 0,
            binary: false,
            truncated: None,
            hunks: vec![Hunk {
                header: format!("@@ -0,0 +1,{lines} @@"),
                lines: (1..=lines)
                    .map(|no| DiffLine {
                        kind: LineKind::Added,
                        old_no: None,
                        new_no: Some(no),
                        text: text.clone(),
                    })
                    .collect(),
            }],
        };
        let snap = Arc::new(DiffSnapshot {
            source: DiffSource::Worktree,
            files: vec![
                file("short.rs", "".into(), 1),
                file("long.rs", "long ".repeat(1000), 100),
            ],
            untracked: vec!["scratch.rs".into()],
            untracked_total: 1,
            ..Default::default()
        });
        app.update_in(&mut vcx, |app, _, cx| {
            let overlay = app.tabs[app.active].diff_overlay.as_mut().unwrap();
            overlay.load = DiffLoad::Ready(snap.clone());
            overlay.preview = Some((
                "scratch.rs".into(),
                Some(Arc::new(file("scratch.rs", "x".into(), 100))),
            ));
            overlay.expanded.insert("short.rs".into(), true);
            overlay.expanded.insert("long.rs".into(), true);
            cx.notify();
        });
        for mode in [DiffViewMode::Unified, DiffViewMode::Split] {
            app.update_in(&mut vcx, |app, _, cx| {
                app.update_config(cx, |cfg| cfg.diff_view = mode)
            });
            for focus in [None, Some("short.rs"), Some("long.rs"), Some("scratch.rs")] {
                app.update_in(&mut vcx, |app, _, cx| {
                    app.tabs[app.active].diff_overlay.as_mut().unwrap().focus =
                        focus.map(str::to_owned);
                    cx.notify();
                });
                for (width, layout) in [
                    (1600., DocumentLayout::Dock),
                    (1100., DocumentLayout::Fill),
                    (760., DocumentLayout::Dock),
                    (1600., DocumentLayout::Dock),
                ] {
                    app.update_in(&mut vcx, |app, _, cx| app.set_document_layout(layout, cx));
                    vcx.simulate_resize(size(px(width), px(900.)));
                    vcx.run_until_parked();
                    let scroll = vcx.debug_bounds("diff-scroll").unwrap();
                    let panel = vcx.debug_bounds("diff-panel").unwrap();
                    assert_eq!(
                        scroll.size.width, panel.size.width,
                        "scroll width in {mode:?} with {focus:?}"
                    );
                    let row = vcx.debug_bounds("diff-columns").unwrap();
                    let mut card_width = None;
                    for selector in ["diff-card-0", "diff-card-1", "diff-preview-card"] {
                        if let Some(card) = vcx.debug_bounds(selector) {
                            assert_eq!(
                                card.size.width,
                                scroll.size.width - px(32.),
                                "card width in {mode:?} with {focus:?}"
                            );
                            card_width = Some(card.size.width);
                        }
                    }
                    assert_eq!(
                        row.size.width,
                        card_width.unwrap() - px(2.),
                        "row width in {mode:?} with {focus:?}"
                    );
                    if mode == DiffViewMode::Split {
                        let old = vcx.debug_bounds("diff-old-scroll").unwrap();
                        let new = vcx.debug_bounds("diff-new-scroll").unwrap();
                        assert!(
                            (old.size.width - new.size.width).abs() <= px(1.),
                            "split cells remain equal"
                        );
                        assert_eq!(old.size.width + new.size.width + px(1.), row.size.width);
                    }
                    if focus != Some("short.rs") {
                        assert!(
                            app.update_in(&mut vcx, |app, _, _| {
                                app.tabs[app.active]
                                    .diff_overlay
                                    .as_ref()
                                    .unwrap()
                                    .scroll
                                    .max_offset()
                                    .y
                                    > px(0.)
                            }),
                            "the long patch retains its scroll range"
                        );
                    }
                }
            }
        }
        // A collapsed focused card still fills the list, including after a
        // refreshed patch replaces the snapshot that was initially displayed.
        for expanded in [false, true] {
            app.update_in(&mut vcx, |app, _, cx| {
                let overlay = app.tabs[app.active].diff_overlay.as_mut().unwrap();
                overlay.focus = Some("short.rs".into());
                overlay.expanded.insert("short.rs".into(), expanded);
                let mut refreshed = snap.as_ref().clone();
                refreshed.files[0].hunks[0].lines[0].text = "refreshed".into();
                overlay.load = DiffLoad::Ready(Arc::new(refreshed));
                cx.notify();
            });
            vcx.run_until_parked();
            let scroll = vcx.debug_bounds("diff-scroll").unwrap();
            let card = vcx.debug_bounds("diff-card-0").unwrap();
            assert_eq!(card.size.width, scroll.size.width - px(32.));
            if expanded {
                assert_eq!(
                    vcx.debug_bounds("diff-columns").unwrap().size.width,
                    card.size.width - px(2.)
                );
            } else {
                assert!(vcx.debug_bounds("diff-columns").is_none());
            }
        }
    }

    #[gpui::test]
    fn compact_diff_button_preserves_the_focused_patch_and_switches_both_ways(
        cx: &mut TestAppContext,
    ) {
        use crate::core::config::DiffViewMode;
        use crate::terminal::git_diff::{
            DiffLine, DiffSnapshot, DiffSource, FileDiff, FileStatus, Hunk, LineKind,
        };
        use crate::ui::diff_overlay::DiffLoad;
        use std::sync::Arc;

        let (app, mut vcx) = window(cx, 1600.);
        let cwd = std::path::PathBuf::from("/no/such/tty7/repo");
        app.update_in(&mut vcx, |app, window, cx| {
            app.open_diff_overlay(
                crate::ui::host_ops::HostId::LOCAL,
                cwd.clone(),
                DiffSource::Head,
                Some("a.rs".into()),
                window,
                cx,
            );
        });
        // The real background probe must finish before installing this patch;
        // parking the GPUI executor alone does not wait for its Git process.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            vcx.run_until_parked();
            if app.update_in(&mut vcx, |app, _, _| {
                !app.tabs[app.active].diff_overlay.as_ref().unwrap().loading
            }) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "diff probe did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let snapshot = Arc::new(DiffSnapshot {
            root: cwd,
            source: DiffSource::Head,
            files: vec![FileDiff {
                path: "a.rs".into(),
                old_path: None,
                status: FileStatus::Modified,
                added: 1,
                removed: 1,
                binary: false,
                truncated: None,
                hunks: vec![Hunk {
                    header: "@@ -1 +1 @@".into(),
                    lines: vec![
                        DiffLine {
                            kind: LineKind::Removed,
                            old_no: Some(1),
                            new_no: None,
                            text: "old".into(),
                        },
                        DiffLine {
                            kind: LineKind::Added,
                            old_no: None,
                            new_no: Some(1),
                            text: "new".into(),
                        },
                    ],
                }],
            }],
            ..Default::default()
        });
        app.update_in(&mut vcx, |app, _, cx| {
            let overlay = app.tabs[app.active].diff_overlay.as_mut().unwrap();
            overlay.load = DiffLoad::Ready(snapshot.clone());
            overlay.loading = false;
            overlay.expanded.insert("a.rs".into(), true);
            cx.notify();
        });
        for expected in [DiffViewMode::Unified, DiffViewMode::Split] {
            vcx.run_until_parked();
            let button = vcx
                .debug_bounds("diff-view-toggle")
                .expect("one visible diff layout button");
            assert_eq!(button.size.width, px(crate::ui::app::TILE_SIZE));
            vcx.simulate_click(button.center(), gpui::Modifiers::none());
            vcx.run_until_parked();
            app.update_in(&mut vcx, |app, _, cx| {
                assert_eq!(cx.global::<Config>().diff_view, expected);
                let overlay = app.tabs[app.active].diff_overlay.as_ref().unwrap();
                assert_eq!(overlay.focus.as_deref(), Some("a.rs"));
                assert_eq!(overlay.expanded.get("a.rs"), Some(&true));
                let DiffLoad::Ready(current) = &overlay.load else {
                    panic!("the patch remains loaded")
                };
                assert!(Arc::ptr_eq(current, &snapshot));
            });
        }
        app.update_in(&mut vcx, |app, window, cx| {
            app.close_diff_overlay(window, cx);
            app.toggle_code_panel(window, cx);
        });
        vcx.run_until_parked();
        assert!(vcx.debug_bounds("document-fill-toggle").is_some());
        assert!(
            vcx.debug_bounds("diff-view-toggle").is_none(),
            "ordinary files have no diff control"
        );
    }

    #[gpui::test]
    fn document_controls_remain_clickable_in_narrow_windows(cx: &mut TestAppContext) {
        let (app, mut vcx) = window(cx, 560.);
        for front in [OverlayTop::Code, OverlayTop::Diff] {
            app.update_in(&mut vcx, |app, window, cx| {
                match front {
                    OverlayTop::Code => app.toggle_code_panel(window, cx),
                    OverlayTop::Diff => app.open_diff_overlay(
                        crate::ui::host_ops::HostId::LOCAL,
                        std::path::PathBuf::from("/no/such/tty7/repo"),
                        crate::terminal::git_diff::DiffSource::Head,
                        None,
                        window,
                        cx,
                    ),
                }
                app.right_panel_visible = false;
                cx.notify();
            });
            for width in [560., 360.] {
                vcx.simulate_resize(size(px(width), px(900.)));
                vcx.run_until_parked();
                for expected in [DocumentLayout::Fill, DocumentLayout::Dock] {
                    let button = vcx
                        .debug_bounds("document-fill-toggle")
                        .expect("layout control stays visible");
                    assert!(
                        button.left() >= px(0.) && button.right() <= px(width),
                        "layout control fits inside the window: {button:?}"
                    );
                    for selector in [
                        "titlebar-right-panel",
                        "titlebar-search",
                        "titlebar-settings",
                    ] {
                        let tile = vcx
                            .debug_bounds(selector)
                            .expect("titlebar actions stay visible");
                        assert!(tile.right() <= px(width - crate::ui::app::WINDOW_CONTROLS_W));
                        assert!(
                            button.right() <= tile.left() || button.top() >= tile.bottom(),
                            "{front:?} at {width}px: document and titlebar controls cannot overlap"
                        );
                    }
                    vcx.simulate_click(button.center(), gpui::Modifiers::none());
                    vcx.run_until_parked();
                    assert_eq!(
                        tab_layout(&app, &mut vcx),
                        expected,
                        "{front:?} at {width}px, button={button:?}"
                    );
                }
                if front == OverlayTop::Diff {
                    let button = vcx
                        .debug_bounds("diff-view-toggle")
                        .expect("diff control stays visible");
                    let before = vcx.update(|_, cx| cx.global::<Config>().diff_view);
                    assert!(button.left() >= px(0.) && button.right() <= px(width));
                    vcx.simulate_click(button.center(), gpui::Modifiers::none());
                    vcx.run_until_parked();
                    assert_ne!(vcx.update(|_, cx| cx.global::<Config>().diff_view), before);
                }
            }
            app.update_in(&mut vcx, |app, window, cx| match front {
                OverlayTop::Code => app.toggle_code_panel(window, cx),
                OverlayTop::Diff => app.close_diff_overlay(window, cx),
            });
        }
    }

    #[gpui::test]
    fn the_existing_panel_entry_remains_clickable_while_documents_fill(cx: &mut TestAppContext) {
        let (app, mut vcx) = window(cx, 1600.);
        for front in [OverlayTop::Code, OverlayTop::Diff] {
            app.update_in(&mut vcx, |app, window, cx| {
                app.set_document_layout(DocumentLayout::Dock, cx);
                match front {
                    OverlayTop::Code => app.toggle_code_panel(window, cx),
                    OverlayTop::Diff => app.open_diff_overlay(
                        crate::ui::host_ops::HostId::LOCAL,
                        std::path::PathBuf::from("/no/such/tty7/repo"),
                        crate::terminal::git_diff::DiffSource::Head,
                        None,
                        window,
                        cx,
                    ),
                }
                // Opening code makes the Files panel visible. Hide it only
                // after opening the document to exercise the covered entry.
                app.right_panel_visible = false;
                cx.notify();
            });
            vcx.run_until_parked();
            assert!(!app.update_in(&mut vcx, |app, _, cx| app.right_panel_open(cx)));
            let entry = vcx
                .debug_bounds("titlebar-right-panel")
                .expect("the original entry is visible");
            assert!(
                entry.left() >= px(1600. - crate::ui::tab_strip::trailing_chrome_w()),
                "the panel entry stays in the window corner, outside the terminal column"
            );
            let fill = vcx
                .debug_bounds("document-fill-toggle")
                .expect("both document headers have fill");
            vcx.simulate_click(fill.center(), gpui::Modifiers::none());
            vcx.run_until_parked();
            assert_eq!(
                vcx.debug_bounds("titlebar-right-panel"),
                Some(entry),
                "the original entry stays put"
            );
            let search = vcx
                .debug_bounds("titlebar-search")
                .expect("command search stays in the document corner");
            let settings = vcx
                .debug_bounds("titlebar-settings")
                .expect("settings stays in the document corner");
            assert_eq!(search.left() - entry.right(), px(2.));
            assert_eq!(settings.left() - search.right(), px(2.));
            for button in [entry, search, settings] {
                assert_eq!(
                    button.size,
                    size(px(crate::ui::app::TILE_SIZE), px(crate::ui::app::TILE_SIZE))
                );
                assert!(button.right() <= px(1600. - crate::ui::app::WINDOW_CONTROLS_W));
                let restore = vcx.debug_bounds("document-fill-toggle").unwrap();
                assert!(
                    restore.right() <= button.left() || restore.top() >= button.bottom(),
                    "the document control cannot overlap a titlebar action"
                );
            }
            vcx.simulate_mouse_move(settings.center(), None, gpui::Modifiers::none());
            vcx.simulate_click(settings.center(), gpui::Modifiers::none());
            vcx.run_until_parked();
            assert!(app.update_in(&mut vcx, |app, _, _| app.active_settings().is_some()));
            vcx.simulate_keystrokes("escape");
            vcx.run_until_parked();
            vcx.simulate_mouse_move(entry.center(), None, gpui::Modifiers::none());
            vcx.simulate_click(entry.center(), gpui::Modifiers::none());
            vcx.run_until_parked();
            assert!(
                app.update_in(&mut vcx, |app, _, cx| app.right_panel_open(cx)),
                "fill must not cover the original entry"
            );
            let restore = vcx
                .debug_bounds("document-fill-toggle")
                .expect("restore remains reachable beside the panel");
            vcx.simulate_click(restore.center(), gpui::Modifiers::none());
            vcx.run_until_parked();
            assert_eq!(tab_layout(&app, &mut vcx), DocumentLayout::Dock);
            app.update_in(&mut vcx, |app, window, cx| match front {
                OverlayTop::Code => app.toggle_code_panel(window, cx),
                OverlayTop::Diff => app.close_diff_overlay(window, cx),
            });
        }
    }

    /// Fill is one tab's answer, not the window's. Reading a long file in one
    /// tab while an agent works in another wants the whole window here and half
    /// of it there, and a global switch made each of those flip the other.
    #[gpui::test]
    fn one_tabs_fill_leaves_the_other_docked(cx: &mut TestAppContext) {
        let (app, mut vcx) = window_with(cx, 1440., 2);

        // A file open in each tab, both docked to start with.
        for i in [0, 1] {
            app.update_in(&mut vcx, |app, window, cx| {
                app.active = i;
                app.toggle_code_panel(window, cx);
            });
        }
        vcx.run_until_parked();

        app.update_in(&mut vcx, |app, _, cx| {
            app.active = 1;
            app.toggle_document_fill(cx);
        });
        vcx.run_until_parked();
        assert_eq!(dock_px(&app, &mut vcx), None, "the tab that asked, fills");

        app.update_in(&mut vcx, |app, _, _cx| app.active = 0);
        vcx.run_until_parked();
        assert!(
            dock_px(&app, &mut vcx).is_some(),
            "the tab that did not ask, does not"
        );

        // The config is untouched — it is what every tab that was never told is
        // still reading, so writing it would have reached all of them at once.
        assert_eq!(layout(&mut vcx), DocumentLayout::Dock);
        app.update_in(&mut vcx, |app, window, cx| {
            push_tab(app, cx);
            app.active = 2;
            app.toggle_code_panel(window, cx);
        });
        vcx.run_until_parked();
        assert!(
            dock_px(&app, &mut vcx).is_some(),
            "a fresh tab starts from the config, not from what tab 1 chose"
        );
    }

    /// A docked column takes width off the tab strip too, and the strip has to
    /// be told. Where the header is hoisted — Windows, Linux — it is drawn over
    /// the trailing end of the spanning title bar with no fill of its own, so a
    /// chip left under it shows through the file name and stays clickable
    /// through it. On macOS the strip sits inside the terminal column, and
    /// sizing it to the window rather than to that column is what pushed the
    /// New Tab button off the end before the detail panel got the same
    /// reservation.
    #[gpui::test]
    fn the_tab_chips_stop_where_the_document_column_starts(cx: &mut TestAppContext) {
        let (app, mut vcx) = window_with(cx, 1200., 10);
        // Chips only exist with the bar along the top; with the rail up the
        // strip is the sidebar's and has nothing in this row to collide with.
        vcx.update(|_, cx| {
            cx.global_mut::<Config>().tab_bar_position = crate::core::config::TabBarPosition::Top;
        });
        app.update_in(&mut vcx, |app, window, cx| {
            for (i, tab) in app.tabs.iter_mut().enumerate() {
                tab.name = Some(format!("a tab with a long enough name {i}"));
            }
            app.active = 0;
            app.toggle_code_panel(window, cx);
        });
        vcx.run_until_parked();

        let (viewport, panel, docked) = app.update_in(&mut vcx, |app, window, cx| {
            (
                window.viewport_size().width.as_f32(),
                if app.right_panel_open(cx) {
                    app.right_panel_px(window, cx)
                } else {
                    0.
                },
                app.document_dock_px(window, cx)
                    .expect("a 1200 window seats both"),
            )
        });
        let column_left = viewport - panel - docked;
        let chips = app.update_in(&mut vcx, |app, _, _| app.strip_slots.borrow().clone());
        let far = chips
            .iter()
            .map(|b| (b.origin.x + b.size.width).as_f32())
            .fold(0., f32::max);
        assert!(far > 0., "the strip has to have drawn chips to measure");
        assert!(
            far <= column_left + 0.5,
            "a chip reached {far}, past the column's left edge at {column_left}"
        );
    }

    /// The palette row has to name what running it will do. Fill is per tab, so
    /// a tab told to fill must be offered "Dock" — even though the config, which
    /// every tab that was never told is still reading, still says dock. Reading
    /// the config here offered "Fill" to a tab that was already filling.
    #[gpui::test]
    fn the_palette_offers_the_active_tabs_layout_not_the_configs(cx: &mut TestAppContext) {
        use crate::ui::i18n::L10nKey;
        use crate::ui::palette::CommandKind;

        let (app, mut vcx) = window_with(cx, 1440., 2);
        crate::ui::i18n::set_locale("en");
        let row = |app: &Entity<Tty7App>, vcx: &mut VisualTestContext| {
            app.update_in(vcx, |app, window, cx| {
                app.palette_commands(window, cx)
                    .into_iter()
                    .find(|c| c.kind == CommandKind::ToggleDocumentFill)
                    .expect("the palette offers the fill toggle")
                    .title
            })
        };
        assert_eq!(row(&app, &mut vcx), t(L10nKey::CmdDocumentFill).to_string());

        app.update_in(&mut vcx, |app, _, cx| {
            app.active = 1;
            app.toggle_document_fill(cx);
        });
        vcx.run_until_parked();
        assert_eq!(
            row(&app, &mut vcx),
            t(L10nKey::CmdDocumentDock).to_string(),
            "a tab that is filling is offered the way back"
        );
        assert_eq!(
            layout(&mut vcx),
            DocumentLayout::Dock,
            "and the config it did not write still reads dock"
        );

        app.update_in(&mut vcx, |app, _, _| app.active = 0);
        vcx.run_until_parked();
        assert_eq!(
            row(&app, &mut vcx),
            t(L10nKey::CmdDocumentFill).to_string(),
            "the other tab is untouched, and is still offered fill"
        );
    }

    /// The complaint in #625: opening a file must leave the terminal on screen.
    /// The column is half of what the terminal and the document share, and the
    /// terminal's own laid-out area gives up exactly that width — which is what
    /// makes the PTY reflow rather than hide half its columns under a card.
    #[gpui::test]
    fn opening_the_code_panel_docks_a_column_beside_the_terminal(cx: &mut TestAppContext) {
        let (app, mut vcx) = window(cx, 1440.);

        app.update_in(&mut vcx, |app, window, cx| {
            app.toggle_code_panel(window, cx);
        });
        vcx.run_until_parked();

        let body = app.update_in(&mut vcx, |app, window, cx| app.document_body_px(window, cx));
        let docked = dock_px(&app, &mut vcx).expect("a 1440 window seats both");
        assert!(
            (docked - body / 2.).abs() < 0.5,
            "half the terminal column, not half the window: {docked} of {body}"
        );

        // The terminal is laid out at what is left, not at the full width with
        // a card over half of it. `pane_area` is the rectangle the grid sizes
        // itself from, so this is the assertion the PTY reflow rests on.
        let pane = app
            .update_in(&mut vcx, |app, _, _| app.pane_area.get())
            .expect("the body area painted");
        assert!(
            (pane.size.width.as_f32() - (body - docked)).abs() < 1.5,
            "terminal laid out at {} of a {body} body beside a {docked} column",
            pane.size.width.as_f32()
        );
        assert!(pane.size.width.as_f32() >= TERMINAL_MIN_W);
    }

    /// Esc — which is what `toggle_code_panel` runs — puts the width back.
    #[gpui::test]
    fn closing_the_surface_gives_the_width_back(cx: &mut TestAppContext) {
        let (app, mut vcx) = window(cx, 1440.);
        app.update_in(&mut vcx, |app, window, cx| {
            app.toggle_code_panel(window, cx);
        });
        vcx.run_until_parked();
        assert!(dock_px(&app, &mut vcx).is_some());

        app.update_in(&mut vcx, |app, window, cx| {
            app.toggle_code_panel(window, cx);
        });
        vcx.run_until_parked();
        assert_eq!(
            app.update_in(&mut vcx, |app, _, _| app.document_front()),
            None
        );
        assert_eq!(dock_px(&app, &mut vcx), None);
        assert_eq!(
            app.update_in(&mut vcx, |app, _, cx| app.document_floor(cx)),
            0.,
            "a closed surface reserves nothing from the panels"
        );
    }

    /// A window with no room for both falls back to the overlay for the frame
    /// and leaves the saved layout alone. Widening re-docks with no command run
    /// in between — the fallback is derived, not stored.
    #[gpui::test]
    fn a_narrow_window_falls_back_without_saving_it(cx: &mut TestAppContext) {
        let (app, mut vcx) = window(cx, 560.);
        app.update_in(&mut vcx, |app, window, cx| {
            app.toggle_code_panel(window, cx);
        });
        vcx.run_until_parked();

        assert_eq!(dock_px(&app, &mut vcx), None, "no room for both");
        assert_eq!(
            layout(&mut vcx),
            DocumentLayout::Dock,
            "the fallback must never write the user's choice"
        );

        vcx.simulate_resize(size(px(1440.), px(900.)));
        vcx.run_until_parked();
        assert!(
            dock_px(&app, &mut vcx).is_some(),
            "widening re-docks on the next frame"
        );
        assert_eq!(layout(&mut vcx), DocumentLayout::Dock);
    }

    /// Filling is still there, and asking for it *is* a choice worth keeping —
    /// including from inside the narrow-window fallback, where the user is
    /// looking at an overlay and saying they want it.
    #[gpui::test]
    fn asking_to_fill_is_kept_and_a_named_width_docks_again(cx: &mut TestAppContext) {
        let (app, mut vcx) = window(cx, 560.);
        app.update_in(&mut vcx, |app, window, cx| {
            app.toggle_code_panel(window, cx);
        });
        app.update_in(&mut vcx, |app, _, cx| app.toggle_document_fill(cx));
        vcx.run_until_parked();
        assert_eq!(tab_layout(&app, &mut vcx), DocumentLayout::Fill);

        vcx.simulate_resize(size(px(1440.), px(900.)));
        vcx.run_until_parked();
        assert_eq!(
            dock_px(&app, &mut vcx),
            None,
            "a wide window does not overrule a chosen fill"
        );

        // Asking for two thirds of the width is asking for a column.
        app.update_in(&mut vcx, |app, _, cx| {
            app.set_document_ratio(DOCUMENT_RATIO_TWO_THIRDS, cx)
        });
        vcx.run_until_parked();
        assert_eq!(tab_layout(&app, &mut vcx), DocumentLayout::Dock);
        let body = app.update_in(&mut vcx, |app, window, cx| app.document_body_px(window, cx));
        let docked = dock_px(&app, &mut vcx).expect("docked again");
        // Two thirds, or as near as the terminal's floor allows — with both
        // side panels open a 1440 window has 960 to share, and two thirds of
        // that would leave the terminal 320.
        let want = (body * 2. / 3.).min(body - TERMINAL_MIN_W);
        assert!((docked - want).abs() < 0.5, "{docked} of {body}");
        assert!(docked > body / 2., "wider than the half it started at");
        assert_eq!(
            vcx.update(|_, cx| cx.global::<Config>().document_ratio),
            DOCUMENT_RATIO_TWO_THIRDS
        );

        // The header menu writes the layout outright rather than toggling it.
        app.update_in(&mut vcx, |app, _, cx| {
            app.set_document_layout(DocumentLayout::Fill, cx)
        });
        vcx.run_until_parked();
        assert_eq!(tab_layout(&app, &mut vcx), DocumentLayout::Fill);
        assert_eq!(dock_px(&app, &mut vcx), None);
        assert_eq!(
            vcx.update(|_, cx| cx.global::<Config>().document_ratio),
            DOCUMENT_RATIO_TWO_THIRDS,
            "filling does not forget the width to come back to"
        );
    }
}
