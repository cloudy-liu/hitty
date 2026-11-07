//! Character selection for diff columns. Display offsets include expanded tabs
//! and diff markers; clipboard offsets always refer to the original UTF-8.

use gpui::{
    App, Bounds, Context, DispatchPhase, Div, Entity, FocusHandle, Focusable, Font, KeyDownEvent,
    MouseButton, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollHandle, ShapedLine,
    SharedString, TextRun, Window, anchored, canvas, deferred, fill, point, prelude::*, px, size,
};
use gpui_component::{
    ActiveTheme as _,
    menu::{PopupMenu, PopupMenuItem},
};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::core::config::DiffViewMode;
use crate::ui::i18n::{L10nKey, t};

#[cfg(test)]
pub(crate) mod geometry_probe {
    thread_local! { static ROWS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
    pub(crate) fn reset() {
        ROWS.set(0);
    }
    pub(crate) fn count() -> usize {
        ROWS.get()
    }
    pub(super) fn record() {
        ROWS.set(ROWS.get() + 1);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Position {
    row: usize,
    byte: usize,
}

#[derive(Clone)]
struct RowGeometry {
    bounds: Bounds<Pixels>,
    line: ShapedLine,
    // A grapheme is indivisible, including a tab expanded to four spaces.
    boundaries: Vec<(usize, usize)>,
}

struct Column {
    lines: Vec<SharedString>,
    geometry: Vec<Option<RowGeometry>>,
    viewport: Option<Bounds<Pixels>>,
    horizontal: Option<ScrollHandle>,
    vertical: Option<ScrollHandle>,
}

pub(crate) struct DiffSelection {
    focus: FocusHandle,
    mode: Option<DiffViewMode>,
    columns: Vec<Column>,
    column: usize,
    anchor: Option<Position>,
    head: Option<Position>,
    pointer: Option<Point<Pixels>>,
    drag_moved: bool,
    drag_task: Option<gpui::Task<()>>,
    menu: Option<(usize, Point<Pixels>, Entity<PopupMenu>)>,
    menu_subscription: Option<gpui::Subscription>,
}

impl DiffSelection {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            mode: None,
            columns: Vec::new(),
            column: 0,
            anchor: None,
            head: None,
            pointer: None,
            drag_moved: false,
            drag_task: None,
            menu: None,
            menu_subscription: None,
        }
    }

    pub(crate) fn configure(&mut self, mode: DiffViewMode, columns: Vec<Vec<SharedString>>) {
        if self.mode != Some(mode)
            || self.columns.len() != columns.len()
            || self
                .columns
                .iter()
                .zip(&columns)
                .any(|(a, b)| a.lines != *b)
        {
            self.mode = Some(mode);
            self.anchor = None;
            self.head = None;
            self.pointer = None;
            self.drag_task = None;
            self.menu = None;
            self.menu_subscription = None;
            self.column = 0;
            self.columns = columns
                .into_iter()
                .map(|lines| Column {
                    geometry: vec![None; lines.len()],
                    lines,
                    viewport: None,
                    horizontal: None,
                    vertical: None,
                })
                .collect();
        }
        for column in &mut self.columns {
            column.geometry.fill(None);
        }
    }

    pub(crate) fn cancel_drag(&mut self) {
        self.pointer = None;
        self.drag_task = None;
    }

    fn range(&self) -> Option<(Position, Position)> {
        let (a, b) = (self.anchor?, self.head?);
        Some((a.min(b), a.max(b)))
    }

    fn text(&self) -> String {
        let Some((start, end)) = self.range() else {
            return String::new();
        };
        self.columns[self.column]
            .lines
            .iter()
            .enumerate()
            .skip(start.row)
            .take(end.row - start.row + 1)
            .map(|(row, text)| {
                let a = if row == start.row { start.byte } else { 0 };
                let b = if row == end.row { end.byte } else { text.len() };
                &text[a..b]
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn copy(&self, cx: &mut App) {
        let text = self.text();
        if !text.is_empty() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
    }

    fn select_all(&mut self, column: usize, cx: &mut Context<Self>) {
        let lines = &self.columns[column].lines;
        if let Some(last) = lines.last() {
            self.column = column;
            self.anchor = Some(Position { row: 0, byte: 0 });
            self.head = Some(Position {
                row: lines.len() - 1,
                byte: last.len(),
            });
            cx.notify();
        }
    }

    fn hit(&self, column: usize, position: Point<Pixels>) -> Option<Position> {
        let col = &self.columns[column];
        let (row, geometry) = col
            .geometry
            .iter()
            .enumerate()
            .filter_map(|(row, g)| g.as_ref().map(|g| (row, g)))
            .min_by(|(_, a), (_, b)| {
                let distance = |g: &RowGeometry| (position.y - g.bounds.center().y).abs().as_f32();
                distance(a).total_cmp(&distance(b))
            })?;
        let x = position.x - geometry.bounds.left();
        let &(byte, _) = geometry.boundaries.iter().min_by(|(_, a), (_, b)| {
            (geometry.line.x_for_index(*a) - x)
                .abs()
                .as_f32()
                .total_cmp(&(geometry.line.x_for_index(*b) - x).abs().as_f32())
        })?;
        Some(Position { row, byte })
    }

    fn start(
        &mut self,
        column: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(at) = self.hit(column, event.position) else {
            return;
        };
        window.focus(&self.focus, cx);
        if !event.modifiers.shift || self.column != column || self.anchor.is_none() {
            self.anchor = Some(at);
        }
        self.column = column;
        self.head = Some(at);
        if event.click_count == 2 {
            let text = &self.columns[column].lines[at.row];
            if let Some((start, word)) = text.split_word_bound_indices().find(|(start, word)| {
                let byte = at.byte.min(text.len().saturating_sub(1));
                *start <= byte && byte < start + word.len()
            }) {
                self.anchor = Some(Position {
                    row: at.row,
                    byte: start,
                });
                self.head = Some(Position {
                    row: at.row,
                    byte: start + word.len(),
                });
            }
        } else if event.click_count >= 3 {
            self.anchor = Some(Position {
                row: at.row,
                byte: 0,
            });
            self.head = Some(Position {
                row: at.row,
                byte: self.columns[column].lines[at.row].len(),
            });
        }
        self.pointer = Some(event.position);
        self.drag_moved = false;
        self.drag_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(16))
                    .await;
                let keep = this.update(cx, |this, cx| {
                    if this.pointer.is_none() {
                        return false;
                    }
                    if this.autoscroll() {
                        cx.notify()
                    }
                    true
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        }));
        cx.notify();
        cx.stop_propagation();
    }

    fn extend(&mut self, pointer: Point<Pixels>, cx: &mut Context<Self>) {
        self.drag_moved |= self.pointer != Some(pointer);
        self.pointer = Some(pointer);
        if let Some(at) = self.hit(self.column, self.clamped_pointer(pointer))
            && self.head != Some(at)
        {
            self.head = Some(at);
            cx.notify();
        }
    }

    fn clamped_pointer(&self, mut pointer: Point<Pixels>) -> Point<Pixels> {
        if let Some(bounds) = self.columns[self.column].viewport {
            pointer.x = pointer.x.clamp(bounds.left(), bounds.right());
            pointer.y = pointer.y.clamp(bounds.top(), bounds.bottom());
        }
        pointer
    }

    fn autoscroll(&mut self) -> bool {
        let Some(pointer) = self.pointer else {
            return false;
        };
        let column = &self.columns[self.column];
        let Some(bounds) = column.viewport else {
            return false;
        };
        let step = |at: Pixels, first: Pixels, last: Pixels| {
            if at < first {
                (first - at).min(px(30.))
            } else if at > last {
                -(at - last).min(px(30.))
            } else {
                px(0.)
            }
        };
        let mut changed = false;
        for (scroll, horizontal) in [(&column.horizontal, true), (&column.vertical, false)] {
            let Some(scroll) = scroll else { continue };
            let before = scroll.offset();
            let mut after = before;
            if horizontal {
                after.x += step(pointer.x, bounds.left(), bounds.right());
            } else {
                after.y += step(pointer.y, bounds.top(), bounds.bottom());
            }
            scroll.set_offset(after);
            changed |= scroll.offset() != before;
        }
        changed
    }

    fn open_menu(
        &mut self,
        column: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let copy = cx.entity().downgrade();
        let all = copy.clone();
        let focus = self.focus.clone();
        let empty = self.text().is_empty();
        let menu = PopupMenu::build(window, cx, |menu, _, _| {
            menu.action_context(focus)
                .item(
                    PopupMenuItem::new(t(L10nKey::AppMenuCopy))
                        .disabled(empty)
                        .on_click(move |_, _, cx| {
                            let _ = copy.update(cx, |s, cx| s.copy(cx));
                        }),
                )
                .item(
                    PopupMenuItem::new(t(L10nKey::AppMenuSelectAll)).on_click(move |_, _, cx| {
                        let _ = all.update(cx, |s, cx| s.select_all(column, cx));
                    }),
                )
        });
        self.menu_subscription = Some(cx.subscribe(&menu, |s, _, _: &gpui::DismissEvent, cx| {
            s.menu = None;
            s.menu_subscription = None;
            cx.notify();
        }));
        window.focus(&menu.focus_handle(cx), cx);
        self.menu = Some((column, position, menu));
        cx.notify();
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) {
            return;
        }
        if event.keystroke.modifiers.secondary() {
            match event.keystroke.key.as_str() {
                "c" => self.copy(cx),
                "a" => self.select_all(self.column, cx),
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        if event.keystroke.modifiers.alt {
            return;
        }
        let Some(mut at) = self.head else { return };
        let lines = &self.columns[self.column].lines;
        let shift = event.keystroke.modifiers.shift;
        match event.keystroke.key.as_str() {
            "left" if !shift && self.anchor != self.head => at = self.range().unwrap().0,
            "right" if !shift && self.anchor != self.head => at = self.range().unwrap().1,
            "left" => {
                if at.byte == 0 && at.row > 0 {
                    at.row -= 1;
                    at.byte = lines[at.row].len();
                } else {
                    at.byte = lines[at.row]
                        .grapheme_indices(true)
                        .map(|(i, _)| i)
                        .take_while(|i| *i < at.byte)
                        .last()
                        .unwrap_or(0);
                }
            }
            "right" => {
                if at.byte == lines[at.row].len() && at.row + 1 < lines.len() {
                    at.row += 1;
                    at.byte = 0;
                } else {
                    at.byte = lines[at.row]
                        .grapheme_indices(true)
                        .map(|(i, _)| i)
                        .find(|i| *i > at.byte)
                        .unwrap_or(lines[at.row].len());
                }
            }
            "home" => at.byte = 0,
            "end" => at.byte = lines[at.row].len(),
            "up" | "down" => {
                let offset = lines[at.row]
                    .grapheme_indices(true)
                    .take_while(|(i, _)| *i < at.byte)
                    .count();
                at.row = if event.keystroke.key == "up" {
                    at.row.saturating_sub(1)
                } else {
                    (at.row + 1).min(lines.len() - 1)
                };
                at.byte = lines[at.row]
                    .grapheme_indices(true)
                    .nth(offset)
                    .map(|(i, _)| i)
                    .unwrap_or(lines[at.row].len());
            }
            _ => return,
        }
        self.head = Some(at);
        if !shift {
            self.anchor = self.head;
        }
        cx.notify();
        cx.stop_propagation();
    }
}

pub(crate) fn selectable_card(card: Div, state: &Entity<DiffSelection>, cx: &App) -> Div {
    let focus = state.read(cx).focus.clone();
    let state = state.clone();
    card.track_focus(&focus)
        .on_key_down(move |event, window, cx| {
            state.update(cx, |s, cx| s.key(event, window, cx));
        })
}

pub(crate) fn selectable_viewport(
    viewport: gpui::Stateful<Div>,
    state: &Entity<DiffSelection>,
    column: usize,
    horizontal: ScrollHandle,
    vertical: Option<ScrollHandle>,
    cx: &App,
) -> gpui::AnyElement {
    let press = state.clone();
    let geometry = state.clone();
    let mouse = state.clone();
    let menu_state = state.clone();
    let popup = state
        .read(cx)
        .menu
        .as_ref()
        .filter(|(col, _, _)| *col == column)
        .map(|(_, position, menu)| {
            deferred(
                anchored()
                    .position(*position)
                    .snap_to_window_with_margin(px(8.))
                    .child(menu.clone()),
            )
            .with_priority(1)
        });
    viewport
        .cursor_text()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            press.update(cx, |s, cx| s.start(column, event, window, cx));
        })
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            menu_state.update(cx, |s, cx| s.open_menu(column, event.position, window, cx));
            cx.stop_propagation();
        })
        .children(popup)
        .child(
            canvas(
                move |_, _, cx| {
                    geometry.update(cx, |s, _| {
                        let col = &mut s.columns[column];
                        // Clip against the outer file-list viewport as well.
                        // A child canvas moves with scrolled content. The
                        // handle keeps the viewport in window coordinates.
                        let mut visible = horizontal.bounds();
                        if let Some(scroll) = &vertical {
                            visible = visible.intersect(&scroll.bounds());
                        }
                        col.viewport = Some(visible);
                        col.horizontal = Some(horizontal);
                        col.vertical = vertical;
                        if column == s.column
                            && s.drag_moved
                            && let Some(pointer) = s.pointer
                        {
                            s.head = s.hit(column, s.clamped_pointer(pointer));
                        }
                    });
                },
                move |_, _, window, _| {
                    let movement = mouse.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            movement.update(cx, |s, cx| {
                                if s.column != column || s.pointer.is_none() {
                                    return;
                                }
                                if event.pressed_button == Some(MouseButton::Left) {
                                    s.extend(event.position, cx);
                                    cx.stop_propagation();
                                } else {
                                    s.pointer = None;
                                    s.drag_task = None;
                                }
                            });
                        }
                    });
                    let release = mouse.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                            release.update(cx, |s, cx| {
                                if s.column == column && s.pointer.is_some() {
                                    // Preserve word/line selection when a click did not move.
                                    if s.pointer != Some(event.position) {
                                        s.extend(event.position, cx);
                                    }
                                    s.pointer = None;
                                    s.drag_task = None;
                                }
                            });
                        }
                    });
                },
            )
            .absolute()
            .inset_0(),
        )
        .into_any_element()
}

/// A label owns its selection separately from the diff content. Build its
/// keyed state during rendering, including when the file card is collapsed.
#[derive(gpui::IntoElement)]
pub(crate) struct SelectableLabel {
    pub(crate) id: SharedString,
    pub(crate) text: SharedString,
    pub(crate) font: Font,
    pub(crate) font_size: Pixels,
}

impl gpui::RenderOnce for SelectableLabel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| DiffSelection::new(cx));
        state.update(cx, |selection, _| {
            selection.configure(DiffViewMode::Unified, vec![vec![self.text.clone()]]);
        });
        let scroll = ScrollHandle::new();
        let display: SharedString = self.text.replace('\t', "    ").into();
        let text = SelectionLine {
            state: state.clone(),
            column: 0,
            row: 0,
            font: self.font.clone(),
            font_size: self.font_size,
        }
        .attach_label(
            gpui::div()
                .w_full()
                .truncate()
                .text_size(self.font_size)
                .font_family(self.font.family),
            display.clone(),
        )
        .child(display);
        selectable_card(
            gpui::div().w_full().min_w_0().child(selectable_viewport(
                gpui::div()
                    .id(self.id)
                    .relative()
                    .w_full()
                    .overflow_hidden()
                    .track_scroll(&scroll)
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(text),
                &state,
                0,
                scroll,
                None,
                cx,
            )),
            &state,
            cx,
        )
    }
}

pub(crate) struct SelectionLine {
    pub(crate) state: Entity<DiffSelection>,
    pub(crate) column: usize,
    pub(crate) row: usize,
    pub(crate) font: Font,
    pub(crate) font_size: Pixels,
}

impl SelectionLine {
    pub(crate) fn attach(self, text: Div, display: SharedString, prefix: usize) -> Div {
        self.attach_inner(text, display, prefix, true)
    }

    fn attach_label(self, text: Div, display: SharedString) -> Div {
        self.attach_inner(text, display, 0, false)
    }

    fn attach_inner(self, text: Div, display: SharedString, prefix: usize, content: bool) -> Div {
        let geometry = self.state.clone();
        let state = self.state;
        let column = self.column;
        let row = self.row;
        text.relative().child(
            gpui::div()
                .debug_selector(move || {
                    let kind = if content { "text" } else { "label" };
                    format!("diff-selection-{kind}-{column}-{row}")
                })
                .absolute()
                .inset_0()
                .child(
                    canvas(
                        move |bounds, window, cx| {
                            #[cfg(test)]
                            if content {
                                geometry_probe::record();
                            }
                            let raw = geometry.read(cx).columns[column].lines[row].clone();
                            let line = window.text_system().shape_line(
                                display.clone(),
                                self.font_size,
                                &[TextRun {
                                    len: display.len(),
                                    font: self.font,
                                    color: window.text_style().color,
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                }],
                                None,
                            );
                            geometry.update(cx, |s, _| {
                                s.columns[column].geometry[row] = Some(RowGeometry {
                                    bounds,
                                    line,
                                    boundaries: display_boundaries(&raw, prefix),
                                })
                            });
                        },
                        move |_, _, window, cx| {
                            let s = state.read(cx);
                            if s.column != column {
                                return;
                            }
                            let Some((start, end)) = s.range() else {
                                return;
                            };
                            if row < start.row || row > end.row {
                                return;
                            }
                            let Some(g) = &s.columns[column].geometry[row] else {
                                return;
                            };
                            let a = if row == start.row { start.byte } else { 0 };
                            let b = if row == end.row {
                                end.byte
                            } else {
                                s.columns[column].lines[row].len()
                            };
                            let x = |byte| {
                                let display =
                                    g.boundaries.iter().find(|(raw, _)| *raw == byte).unwrap().1;
                                g.line.x_for_index(display)
                            };
                            let left = g.bounds.left() + x(a);
                            if start == end {
                                if s.focus.is_focused(window) {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(left, g.bounds.top()),
                                            size(px(1.), g.bounds.size.height),
                                        ),
                                        cx.theme().caret,
                                    ));
                                }
                                return;
                            }
                            let right = g.bounds.left()
                                + x(b)
                                + if row < end.row { px(5.) } else { px(0.) };
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(left, g.bounds.top()),
                                    size(right - left, g.bounds.size.height),
                                ),
                                cx.theme().selection,
                            ));
                        },
                    )
                    .size_full(),
                ),
        )
    }
}

fn display_boundaries(raw: &str, prefix: usize) -> Vec<(usize, usize)> {
    let mut display = prefix;
    let mut boundaries = Vec::new();
    for (byte, grapheme) in raw.grapheme_indices(true) {
        boundaries.push((byte, display));
        display += if grapheme == "\t" { 4 } else { grapheme.len() };
    }
    boundaries.push((raw.len(), display));
    boundaries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn cancelling_a_hidden_drag_keeps_the_selection(cx: &mut gpui::TestAppContext) {
        let selection = cx.new(DiffSelection::new);
        selection.update(cx, |s, cx| {
            s.configure(DiffViewMode::Unified, vec![vec!["selected".into()]]);
            s.anchor = Some(Position { row: 0, byte: 0 });
            s.head = Some(Position { row: 0, byte: 8 });
            s.pointer = Some(point(px(0.), px(0.)));
            s.drag_task = Some(cx.spawn(async move |_, _| std::future::pending::<()>().await));
            s.cancel_drag();
            assert!(s.pointer.is_none());
            assert!(s.drag_task.is_none());
            assert_eq!(s.text(), "selected");
        });
    }

    #[gpui::test]
    fn partial_multiline_selection_preserves_original_unicode_and_tabs(
        cx: &mut gpui::TestAppContext,
    ) {
        let selection = cx.new(DiffSelection::new);
        selection.update(cx, |s, _| {
            s.configure(
                DiffViewMode::Unified,
                vec![vec!["before 中👩‍💻\tend".into(), "\tsecond after".into()]],
            );
            s.anchor = Some(Position {
                row: 0,
                byte: "before ".len(),
            });
            s.head = Some(Position {
                row: 1,
                byte: "\tsecond".len(),
            });
            assert_eq!(s.text(), "中👩‍💻\tend\n\tsecond");
            std::mem::swap(&mut s.anchor, &mut s.head);
            assert_eq!(s.text(), "中👩‍💻\tend\n\tsecond");
            s.configure(DiffViewMode::Unified, vec![vec!["tiny".into()]]);
            assert_eq!(
                s.text(),
                "",
                "a shorter snapshot must invalidate old byte offsets"
            );
            s.anchor = Some(Position { row: 0, byte: 0 });
            s.head = Some(Position { row: 0, byte: 4 });
            s.configure(DiffViewMode::Split, vec![vec!["changed".into()]]);
            assert_eq!(s.text(), "");
        });
    }

    #[test]
    fn display_offsets_keep_tabs_and_combining_graphemes_atomic() {
        assert_eq!(
            display_boundaries("\te\u{301}中👩‍💻", 4),
            vec![(0, 4), (1, 8), (4, 11), (7, 14), (18, 25)]
        );
    }
}
