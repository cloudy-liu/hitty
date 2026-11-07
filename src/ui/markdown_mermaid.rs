//! Mermaid diagram rendering and native viewport controls.

use crate::core::markdown_theme::{Color, Theme};
use crate::ui::i18n::{L10nKey, t};
use gpui::{
    App, AvailableSpace, Context, ImageSource, MouseButton, ObjectFit, Pixels, Point, Render,
    ScrollDelta, ScrollHandle, ScrollWheelEvent, SharedString, StyledImage as _, Window, canvas,
    div, img, point, prelude::*, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    scroll::{Scrollbar, ScrollbarShow},
};
use merman::render::{
    HeadlessRenderer, HostThemeAppearance, HostThemeOutput, HostThemeProfile, HostThemeRoles,
    HostThemeRootBackground,
};
use std::sync::{Arc, Mutex};

pub(super) fn aspect_ratio(svg: &str) -> f32 {
    let ratio = || {
        let doc = resvg::usvg::roxmltree::Document::parse(svg).ok()?;
        let mut values = doc
            .root_element()
            .attribute("viewBox")?
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty());
        let width: f32 = values.nth(2)?.parse().ok()?;
        let height: f32 = values.next()?.parse().ok()?;
        let ratio = width / height;
        (width > 0. && height > 0. && ratio.is_finite() && ratio > 0.).then_some(ratio)
    };
    ratio().unwrap_or(2.)
}

// One retained RGBA raster per viewer: at most 64 MiB and 8192px on either axis.
fn raster_scale(bytes: &[u8], requested: f32) -> f32 {
    let dimensions = || {
        let source = std::str::from_utf8(bytes).ok()?;
        let doc = resvg::usvg::roxmltree::Document::parse(source).ok()?;
        let mut values = doc
            .root_element()
            .attribute("viewBox")?
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .skip(2);
        let width: f32 = values.next()?.parse().ok()?;
        let height: f32 = values.next()?.parse().ok()?;
        (width > 0. && height > 0. && width.is_finite() && height.is_finite())
            .then_some((width, height))
    };
    let Some((width, height)) = dimensions() else {
        return requested;
    };
    let smooth = gpui::SMOOTH_SVG_SCALE_FACTOR;
    requested
        .min(8192. / width.max(height) / smooth)
        .min((16_777_216. / (width * height)).sqrt() / smooth)
}

fn dpi_scaled_svg(image: &ImageSource, display_scale: f32) -> ImageSource {
    let ImageSource::Image(svg) = image else {
        return image.clone();
    };
    if svg.format != gpui::ImageFormat::Svg {
        return image.clone();
    }

    let bytes: Arc<[u8]> = svg.bytes.clone().into();
    let scale_limit = raster_scale(&bytes, f32::MAX);
    let cached = Arc::new(Mutex::new(None::<(u32, Arc<gpui::RenderImage>)>));
    ImageSource::Custom(Arc::new(move |window, cx| {
        let scale = (window.scale_factor() * display_scale).min(scale_limit);
        let key = scale.to_bits();
        let mut cached = cached.lock().unwrap();
        if let Some((cached_key, image)) = cached.as_ref()
            && *cached_key == key
        {
            return Some(Ok(image.clone()));
        }
        let image = match cx.svg_renderer().render_single_frame(&bytes, scale) {
            Ok(image) => image,
            Err(error) => return Some(Err(gpui::ImageCacheError::from(error))),
        };
        *cached = Some((key, image.clone()));
        Some(Ok(image))
    }))
}

pub(crate) fn open_diagram(
    image: ImageSource,
    code: SharedString,
    aspect: f32,
    window: &mut Window,
    cx: &mut App,
) -> gpui::Entity<DiagramViewer> {
    let viewer = cx.new(|_| DiagramViewer::new(image, code, aspect, true));
    let dialog_viewer = viewer.clone();
    window.open_dialog(cx, move |dialog, window, _| {
        let viewer = dialog_viewer.clone();
        dialog
            .width((window.viewport_size().width * 0.9).min(px(1300.)))
            .title("Mermaid")
            .content(move |content, _, _| content.child(viewer.clone()))
    });
    let focus = viewer.update(cx, |viewer, cx| {
        viewer
            .focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone()
    });
    window.focus(&focus, cx);
    viewer
}

pub(crate) struct DiagramViewer {
    image: ImageSource,
    display_image: ImageSource,
    raster_display_scale: f32,
    code: SharedString,
    aspect: f32,
    expanded: bool,
    zoom: f32,
    pan: Point<Pixels>,
    scroll: ScrollHandle,
    width: Pixels,
    image_width: Pixels,
    natural_width: Pixels,
    height: Pixels,
    drag: Option<Point<Pixels>>,
    drag_start: Point<Pixels>,
    dragged: bool,
    focus: Option<gpui::FocusHandle>,
    viewport: gpui::Bounds<Pixels>,
    #[cfg(any(test, feature = "markdown-visual-tests"))]
    pub(super) rendered_transform: (f32, Point<Pixels>),
    #[cfg(any(test, feature = "markdown-visual-tests"))]
    rendered_image: gpui::Bounds<Pixels>,
}

impl DiagramViewer {
    pub(super) fn new(image: ImageSource, code: SharedString, aspect: f32, expanded: bool) -> Self {
        let display_image = dpi_scaled_svg(&image, 1.);
        Self {
            image,
            display_image,
            raster_display_scale: 1.,
            code,
            aspect,
            expanded,
            zoom: 1.,
            pan: point(px(0.), px(0.)),
            scroll: ScrollHandle::new(),
            width: px(0.),
            image_width: px(0.),
            natural_width: px(0.),
            height: px(0.),
            drag: None,
            drag_start: Default::default(),
            dragged: false,
            focus: None,
            viewport: Default::default(),
            #[cfg(any(test, feature = "markdown-visual-tests"))]
            rendered_transform: (1., point(px(0.), px(0.))),
            #[cfg(any(test, feature = "markdown-visual-tests"))]
            rendered_image: Default::default(),
        }
    }

    #[cfg(feature = "markdown-visual-tests")]
    pub(crate) fn prepare_profile(&mut self, cx: &mut Context<Self>) {
        self.zoom = 4.;
        let max = self.scroll_limit();
        self.scroll.set_offset(-max / 2.);
        cx.notify();
    }

    #[cfg(feature = "markdown-visual-tests")]
    pub(crate) fn profile_bounds(&self) -> gpui::Bounds<Pixels> {
        self.viewport
    }

    #[cfg(feature = "markdown-visual-tests")]
    pub(crate) fn profile_pan(&self) -> Point<Pixels> {
        self.offset()
    }

    #[cfg(feature = "markdown-visual-tests")]
    pub(crate) fn verify_profile_frame(&self) {
        assert_eq!(
            self.rendered_transform,
            (self.zoom, self.offset()),
            "the drawn transform must follow the input; a stale cached frame is not a fast frame"
        );
        let expected = self.viewport.origin
            + self.offset()
            + point(
                (self.width - self.image_width * self.zoom).max(px(0.)) / 2.,
                px(0.),
            );
        assert!((self.rendered_image.origin.x - expected.x).abs() < px(1.));
        assert!((self.rendered_image.origin.y - expected.y).abs() < px(1.));
    }

    fn offset(&self) -> Point<Pixels> {
        if self.expanded {
            self.scroll.offset() + self.pan
        } else {
            self.pan
        }
    }

    fn image_height(&self) -> Pixels {
        self.image_width / self.aspect
    }

    fn scroll_limit(&self) -> Point<Pixels> {
        point(
            (self.image_width * self.zoom - self.width).max(px(0.)),
            (self.image_height() * self.zoom - self.height).max(px(0.)),
        )
    }

    fn set_scroll_offset(&mut self, offset: Point<Pixels>) {
        let max = self.scroll_limit();
        self.pan = point(
            if max.x == px(0.) {
                offset.x.clamp(-self.width / 2., self.width / 2.)
            } else {
                px(0.)
            },
            if max.y == px(0.) {
                offset.y.clamp(-self.height / 2., self.height / 2.)
            } else {
                px(0.)
            },
        );
        self.scroll.set_offset(point(
            offset.x.clamp(-max.x, px(0.)),
            offset.y.clamp(-max.y, px(0.)),
        ));
    }

    fn pan_by(&mut self, delta: Point<Pixels>) -> bool {
        if self.expanded {
            let old = self.offset();
            self.set_scroll_offset(old + delta);
            return self.offset() != old;
        }
        let old = self.pan;
        // Keep fitted diagrams movable without losing the zoomed edge limits.
        let max_x = (self.image_width * (self.zoom - 1.) / 2.).max(self.image_width / 2.);
        let max_y = (self.height * (self.zoom - 1.) / 2.).max(self.height / 2.);
        self.pan = point(
            (self.pan.x + delta.x).clamp(-max_x, max_x),
            (self.pan.y + delta.y).clamp(-max_y, max_y),
        );
        self.pan != old
    }

    fn zoom_by(&mut self, factor: f32) {
        self.zoom_at(factor, self.viewport.center());
    }

    fn zoom_at(&mut self, factor: f32, position: Point<Pixels>) {
        let previous = self.zoom;
        let anchor = position - self.viewport.origin;
        let image_point = (anchor
            - self.offset()
            - point(
                (self.width - self.image_width * previous).max(px(0.)) / 2.,
                px(0.),
            ))
            / previous;
        self.zoom = (self.zoom * factor).clamp(0.25, 8.);
        if self.expanded {
            self.pan = point(px(0.), px(0.));
            self.set_scroll_offset(
                anchor
                    - image_point * self.zoom
                    - point(
                        (self.width - self.image_width * self.zoom).max(px(0.)) / 2.,
                        px(0.),
                    ),
            );
            return;
        }
        let anchor = position - self.viewport.center();
        self.pan = anchor + (self.pan - anchor) * (self.zoom / previous);
        self.pan_by(point(px(0.), px(0.)));
    }

    fn reset(&mut self) {
        self.zoom = 1.;
        self.pan = point(px(0.), px(0.));
        self.scroll.set_offset(point(px(0.), px(0.)));
    }

    fn fit_all(&mut self) {
        self.reset();
        self.zoom = (self.width / self.image_width)
            .min(self.height / self.image_height())
            .min(1.);
    }

    fn on_key_down(&mut self, event: &gpui::KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.alt
        {
            return;
        }
        let key = event.keystroke.key.as_str();
        match key {
            "+" | "=" => self.zoom_by(1.25),
            "-" => self.zoom_by(1. / 1.25),
            "home" if self.expanded => self
                .scroll
                .set_offset(point(self.scroll.offset().x, px(0.))),
            "0" => self.reset(),
            "f" if self.expanded => self.fit_all(),
            "home" if self.zoom != 1. => self.reset(),
            "up" if self.expanded || self.zoom > 1. => {
                self.pan_by(point(px(0.), px(64.)));
            }
            "down" if self.expanded || self.zoom > 1. => {
                self.pan_by(point(px(0.), px(-64.)));
            }
            "left" if self.expanded || self.zoom > 1. => {
                self.pan_by(point(px(64.), px(0.)));
            }
            "right" if self.expanded || self.zoom > 1. => {
                self.pan_by(point(px(-64.), px(0.)));
            }
            "pageup" if self.expanded => {
                self.pan_by(point(px(0.), self.height * 0.9));
            }
            "pagedown" | "space" if self.expanded => {
                self.pan_by(point(px(0.), -self.height * 0.9));
            }
            "end" if self.expanded => {
                self.pan_by(point(px(0.), px(-1_000_000.)));
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn on_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let delta = match event.delta {
            ScrollDelta::Pixels(delta) => delta,
            ScrollDelta::Lines(delta) => point(px(delta.x * 20.), px(delta.y * 20.)),
        };
        if delta.y != px(0.)
            && ((self.expanded && !event.modifiers.shift)
                || event.modifiers.control
                || event.modifiers.platform)
        {
            self.zoom_at(1.1_f32.powf(f32::from(delta.y) / 40.), event.position);
        } else if self.expanded {
            let delta = if event.modifiers.shift && delta.x == px(0.) {
                point(delta.y, px(0.))
            } else {
                delta
            };
            if !self.pan_by(delta) && !self.expanded {
                return;
            }
        } else {
            return;
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn control(
        id: &'static str,
        icon: impl Into<Icon>,
        label: &'static str,
        compact_size: Option<Pixels>,
        cx: &App,
    ) -> Button {
        let theme = super::markdown_preview::current(cx).theme;
        let palette = theme.palette(cx.theme().mode.is_dark());
        Button::new(id)
            .debug_selector(move || id.into())
            .icon(icon)
            .ghost()
            .border_1()
            .bg(super::markdown_preview::color(palette.code_background))
            .border_color(super::markdown_preview::color(palette.border))
            .text_color(super::markdown_preview::color(palette.muted))
            .tooltip(label)
            .when_some(compact_size, |button, size| button.with_size(size))
    }
}

impl Render for DiagramViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        let scale = window.rem_size() / px(16.);
        if self.natural_width == px(0.) {
            let mut probe = img(self.image.clone()).into_any_element();
            self.natural_width = probe
                .layout_as_root(AvailableSpace::min_size(), window, cx)
                .width;
        }
        let width = if self.width > px(0.) {
            self.width
        } else {
            window.viewport_size().width - px(64.)
        };
        let available = (width - px(32. * scale)).max(px(64. * scale));
        let image_width = if self.natural_width > px(0.) {
            available.min(self.natural_width * scale)
        } else {
            available
        };
        self.image_width = image_width;
        // The toolbar and the gesture hint share one row above the diagram so
        // that nothing is drawn over the image. Narrow viewers shrink the
        // buttons so the three of them still fit beside the hint.
        let mut controls_scale = scale.min(width / px(144.));
        if self.expanded {
            // Keep the toolbar row to at most a third of the dialog budget so a
            // large interface font in a short window still leaves room for the
            // diagram.
            let budget = (window.viewport_size().height * 0.9 - px(50.) - window.rem_size() * 1.5)
                .max(px(1.));
            controls_scale = controls_scale.min(budget / px(120.));
        }
        let toolbar_height = px(40. * controls_scale);
        let compact_size = (controls_scale < scale).then_some(px(32. * controls_scale));
        self.height = if self.expanded {
            // The dialog starts 10% down the window and adds 32px padding,
            // two border pixels, a one-rem title and a half-rem gap, then the
            // toolbar row. Keep another 16px below it so the scrollbar stays
            // visible.
            let max_height = (window.viewport_size().height * 0.9
                - px(50.)
                - window.rem_size() * 1.5
                - toolbar_height)
                .min(window.viewport_size().height * 0.75 - toolbar_height)
                .max(px(1.));
            (image_width / self.aspect).clamp(px(200. * scale).min(max_height), max_height)
        } else {
            (image_width / self.aspect).max(px(200. * scale))
        };
        let reset = Self::control(
            "diagram-reset",
            Icon::default().path("icons/diagram-reset.svg"),
            t(L10nKey::Reset),
            compact_size,
            cx,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            cx.stop_propagation();
            this.reset();
            cx.notify();
        }));
        let toolbar = div()
            .flex()
            .gap(px(8. * controls_scale))
            .child(reset)
            .when(self.expanded, |el| {
                el.child(
                    Self::control(
                        "diagram-fit",
                        Icon::default().path("icons/diagram-expand.svg"),
                        t(L10nKey::MarkdownFitDiagram),
                        compact_size,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.fit_all();
                        cx.notify();
                    })),
                )
            })
            .when(!self.expanded, |el| {
                el.child(
                    Self::control(
                        "expand-diagram",
                        Icon::default().path("icons/diagram-expand.svg"),
                        t(L10nKey::MarkdownExpandDiagram),
                        compact_size,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        open_diagram(
                            this.image.clone(),
                            this.code.clone(),
                            this.aspect,
                            window,
                            cx,
                        );
                    })),
                )
            })
            .child(
                Self::control(
                    "copy-diagram",
                    IconName::Copy,
                    t(L10nKey::EditorCopyCode),
                    compact_size,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(this.code.to_string()));
                })),
            );
        let owner = cx.weak_entity();
        #[cfg(any(test, feature = "markdown-visual-tests"))]
        let painted_owner = owner.clone();
        let wheel_owner = owner.clone();
        let expanded = self.expanded;
        let image_height = self.image_height() * self.zoom;
        let scaled_width = image_width * self.zoom;
        let display_scale = if self.natural_width > px(0.) {
            scaled_width / self.natural_width
        } else {
            1.
        };
        if self.raster_display_scale != display_scale {
            self.raster_display_scale = display_scale;
            self.display_image = dpi_scaled_svg(&self.image, display_scale);
        }
        let content = div()
            .id("diagram-content")
            .debug_selector(|| "diagram-content".into())
            .absolute()
            .left(if expanded {
                self.pan.x + (width - scaled_width).max(px(0.)) / 2.
            } else {
                self.pan.x + (width - scaled_width) / 2.
            })
            .top(if expanded {
                self.pan.y
            } else {
                self.pan.y + (self.height - image_height) / 2.
            })
            .w(scaled_width)
            .h(image_height)
            .child(
                img(self.display_image.clone())
                    .id("diagram-image")
                    .debug_selector(|| "diagram-image".into())
                    .absolute()
                    .top_0()
                    .left_0()
                    .min_w_0()
                    .min_h_0()
                    .w(scaled_width)
                    .h(image_height)
                    .object_fit(ObjectFit::Contain),
            );
        #[cfg(any(test, feature = "markdown-visual-tests"))]
        let content = content.child(
            canvas(
                move |bounds, _, cx| {
                    let _ = painted_owner.update(cx, |this, _| {
                        this.rendered_image = bounds;
                        this.rendered_transform = (this.zoom, this.offset());
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        );
        let content = if expanded {
            div()
                .id("diagram-scroll")
                .size_full()
                .overflow_scroll()
                .track_scroll(&self.scroll)
                .child(
                    div()
                        .relative()
                        .flex_none()
                        .w(scaled_width.max(width))
                        .h(image_height.max(self.height))
                        .child(content),
                )
                .into_any_element()
        } else {
            content.into_any_element()
        };
        let viewport = div()
            .id("diagram-viewport")
            .debug_selector(|| "diagram-viewport".into())
            .relative()
            .track_focus(&focus)
            .key_context("DiagramViewer")
            .cursor(if self.drag.is_some() {
                gpui::CursorStyle::ClosedHand
            } else {
                gpui::CursorStyle::OpenHand
            })
            .w_full()
            .h(self.height)
            .overflow_hidden()
            .when(self.expanded, |el| el.bg(cx.theme().tokens.background))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.drag = Some(event.position);
                    this.drag_start = event.position;
                    this.dragged = false;
                    window.focus(this.focus.as_ref().unwrap(), cx);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if event.pressed_button != Some(MouseButton::Left) {
                    this.drag = None;
                    return;
                }
                if let Some(previous) = this.drag {
                    let distance = event.position - this.drag_start;
                    this.dragged |= distance.x.abs() > px(3.) || distance.y.abs() > px(3.);
                    this.drag = Some(event.position);
                    let moved = this.pan_by(event.position - previous);
                    cx.stop_propagation();
                    if moved {
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.drag = None;
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(Self::on_key_down))
            .when(!self.expanded, |el| {
                el.on_click(cx.listener(|this, _, window, cx| {
                    if !this.dragged {
                        cx.stop_propagation();
                        open_diagram(
                            this.image.clone(),
                            this.code.clone(),
                            this.aspect,
                            window,
                            cx,
                        );
                    }
                }))
            })
            .when(!expanded, |el| {
                el.on_scroll_wheel(cx.listener(|this, event, _, cx| this.on_wheel(event, cx)))
            })
            .child(content)
            .when(expanded, |el| {
                el.child(
                    div().absolute().top_0().left_0().size_full().child(
                        Scrollbar::new(&self.scroll)
                            .id("diagram-scrollbar")
                            .scrollbar_show(ScrollbarShow::Always),
                    ),
                )
            })
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.viewport = bounds;
                            if this.width != bounds.size.width {
                                this.width = bounds.size.width;
                                cx.notify();
                            }
                        });
                    },
                    move |bounds, _, window, _| {
                        if expanded {
                            let owner = wheel_owner.clone();
                            // Capture modifier/shift input before GPUI's default scroll
                            // listener so one event changes the shared offset only once.
                            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, _, cx| {
                                if phase == gpui::DispatchPhase::Capture
                                    && bounds.contains(&event.position)
                                {
                                    let _ = owner.update(cx, |this, cx| this.on_wheel(event, cx));
                                }
                            });
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        let theme = super::markdown_preview::current(cx).theme;
        let palette = theme.palette(cx.theme().mode.is_dark());
        let label = if expanded {
            L10nKey::MarkdownDiagramExpandedHint
        } else if cfg!(target_os = "macos") {
            L10nKey::MarkdownDiagramInlineMacHint
        } else {
            L10nKey::MarkdownDiagramInlineHint
        };
        // The hint is removed while dragging so the row stays quiet during
        // movement; otherwise it appears while the block is hovered or the
        // diagram has keyboard focus.
        let hint = div().flex_1().min_w_0().when(self.drag.is_none(), |el| {
            el.child(
                div()
                    .id("diagram-gesture-hint")
                    .debug_selector(|| "diagram-gesture-hint".into())
                    .w_full()
                    .text_color(super::markdown_preview::color(palette.muted))
                    .text_size(px(12. * controls_scale))
                    .truncate()
                    .invisible()
                    .when(focus.contains_focused(window, cx), |hint| hint.visible())
                    .group_hover("diagram-block", |style| style.visible())
                    .child(t(label)),
            )
        });
        div()
            .id("diagram-block")
            .debug_selector(|| "diagram-block".into())
            .group("diagram-block")
            .flex()
            .flex_col()
            .w_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8. * controls_scale))
                    .w_full()
                    .h(toolbar_height)
                    .px(px(8. * controls_scale))
                    .child(hint)
                    .child(
                        div()
                            .id("diagram-toolbar")
                            .debug_selector(|| "diagram-toolbar".into())
                            .flex_none()
                            .child(toolbar),
                    ),
            )
            .child(viewport)
    }
}

/// Render Mermaid `source` to an SVG string styled with `theme`'s palette.
pub fn render_mermaid(source: &str, theme: &Theme, dark: bool) -> Result<String, String> {
    let mut renderer = HeadlessRenderer::new();
    if theme.id == crate::core::markdown_theme::DEFAULT_ID
        && renderer
            .parse_metadata_sync(source)
            .map_err(|err| err.to_string())?
            .is_some_and(|meta| meta.diagram_type.starts_with("flowchart"))
    {
        renderer = renderer.with_text_measurer(Arc::new(GithubFlowchartText::default()));
    }
    renderer
        .render_svg_with_host_theme_sync(source, &build_theme_profile(theme, dark))
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "not a recognized Mermaid diagram".to_string())
}

/// Match GitHub's single-line labels until the label would escape its capped
/// box, then use the renderer's normal wrapping metrics for long labels.
#[derive(Default)]
struct GithubFlowchartText(merman_render::text::VendoredFontMetricsTextMeasurer);

impl merman::render::TextMeasurer for GithubFlowchartText {
    fn measure(
        &self,
        text: &str,
        style: &merman_render::text::TextStyle,
    ) -> merman_render::text::TextMetrics {
        self.0.measure(text, style)
    }

    fn measure_wrapped(
        &self,
        text: &str,
        style: &merman_render::text::TextStyle,
        max_width: Option<f64>,
        mode: merman_render::text::WrapMode,
    ) -> merman_render::text::TextMetrics {
        if mode != merman_render::text::WrapMode::HtmlLike {
            return self.0.measure_wrapped(text, style, max_width, mode);
        }
        let natural = self.0.measure_wrapped(text, style, None, mode);
        // Mermaid adds 30px of horizontal padding on each side of flowchart nodes.
        if max_width.is_some_and(|max_width| natural.width > max_width + 60.) {
            return self.0.measure_wrapped(text, style, max_width, mode);
        }
        let mut metrics = natural;
        if let Some(max_width) = max_width {
            metrics.width = metrics.width.min(max_width);
        }
        metrics
    }

    fn measure_wrapped_raw(
        &self,
        text: &str,
        style: &merman_render::text::TextStyle,
        max_width: Option<f64>,
        mode: merman_render::text::WrapMode,
    ) -> merman_render::text::TextMetrics {
        self.0.measure_wrapped_raw(text, style, max_width, mode)
    }
}

/// Map the Markdown theme palette onto merman's semantic theme roles.
fn build_theme_profile(theme: &Theme, dark: bool) -> HostThemeProfile {
    if theme.id == crate::core::markdown_theme::DEFAULT_ID {
        // GitHub uses Mermaid's own default/dark theme, rather than recoloring
        // diagram nodes with the surrounding Markdown paper palette.
        let mut output = HostThemeOutput::resvg_safe_editor();
        // Portable SVG output otherwise bakes in a white canvas in dark mode.
        // Keep Mermaid's node palette, but let the app paint the reading surface.
        output.root_background = HostThemeRootBackground::Color("transparent".into());
        return HostThemeProfile::builder()
            .output(output)
            .site_config("theme", if dark { "dark" } else { "default" })
            .site_config("flowchart", serde_json::json!({ "diagramPadding": 48 }))
            .site_config("sequence", serde_json::json!({ "diagramMarginY": 40 }))
            .build();
    }
    let p = theme.palette(dark);
    let hex = |c: &Color| Some(color_to_hex(c));

    HostThemeProfile::builder()
        .output(HostThemeOutput::resvg_safe_editor())
        .appearance(if dark {
            HostThemeAppearance::Dark
        } else {
            HostThemeAppearance::Light
        })
        .roles(HostThemeRoles {
            canvas: hex(&p.background),
            surface: hex(&p.paper),
            surface_alt: None,
            surface_muted: None,
            text: hex(&p.foreground),
            subtle_text: hex(&p.quote_foreground),
            border: hex(&p.border),
            line: hex(&p.border),
            edge_label_background: hex(&p.paper),
            cluster_background: hex(&p.paper),
            cluster_border: hex(&p.border),
            note_background: hex(&p.quote_background),
            note_border: hex(&p.quote_border),
            note_text: hex(&p.quote_foreground),
            actor_background: hex(&p.code_background),
            actor_border: hex(&p.border),
            actor_text: hex(&p.foreground),
            activation_background: hex(&p.accent),
            activation_border: hex(&p.accent),
            // Semantic colors stay fixed across themes.
            error: Some("#EF4444".to_string()),
            warning: Some("#F59E0B".to_string()),
            success: Some("#10B981".to_string()),
        })
        .build()
}

fn color_to_hex(color: &Color) -> String {
    let [r, g, b, a] = color.0.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    if a == u8::MAX {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mermaid_node_labels_stay_inside_their_shapes() {
        use resvg::usvg::{Group, Node};
        fn shape_bounds(group: &Group) -> Option<resvg::usvg::tiny_skia_path::Rect> {
            group.children().iter().find_map(|node| match node {
                Node::Path(path) => Some(path.abs_bounding_box()),
                Node::Group(group) => shape_bounds(group),
                _ => None,
            })
        }
        fn number(node: resvg::usvg::roxmltree::Node<'_, '_>, name: &str) -> f64 {
            node.attribute(name)
                .unwrap_or_else(|| panic!("text must have {name}={:?}", node))
                .parse()
                .unwrap_or_else(|error| panic!("text {name} must be numeric: {error}"))
        }
        fn view_box_origin(doc: &resvg::usvg::roxmltree::Document<'_>) -> (f64, f64) {
            let values = doc
                .root_element()
                .attribute("viewBox")
                .expect("Mermaid SVG must have a viewBox")
                .split_whitespace()
                .map(|value| {
                    value
                        .parse::<f64>()
                        .expect("viewBox values must be numeric")
                })
                .collect::<Vec<_>>();
            assert!(values.len() == 4, "viewBox must contain four values");
            (-values[0], -values[1])
        }
        fn normalized_text(node: resvg::usvg::roxmltree::Node<'_, '_>) -> String {
            node.descendants()
                .filter(|child| child.is_text())
                .filter_map(|child| child.text())
                .collect::<String>()
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect()
        }
        let options = resvg::usvg::Options::default();
        let measurer = GithubFlowchartText::default();
        let style = merman_render::text::TextStyle::default();
        for label in [
            "Review the complete installation instructions and verify the device configuration before continuing",
            "设备工程师：绑定或校准基本资料并完成设备信息核对",
            "service-to-do-apk 远端构建、install-apk 安装、日志归档",
        ] {
            for (open, close) in [("[", "]"), ("(", ")"), ("{", "}")] {
                let source = format!("flowchart TD\n A{open}\"{label}\"{close} --> B[End]");
                let svg = render_mermaid(&source, &crate::core::markdown_theme::builtin(), false)
                    .unwrap();
                let doc = resvg::usvg::roxmltree::Document::parse(&svg).unwrap();
                let (view_box_x, view_box_y) = view_box_origin(&doc);
                let id = doc
                    .descendants()
                    .find_map(|node| {
                        node.attribute("id")
                            .filter(|id| id.contains("flowchart-A-"))
                    })
                    .unwrap();
                let tree = resvg::usvg::Tree::from_str(&svg, &options).unwrap();
                let Node::Group(node) = tree.node_by_id(id).unwrap() else {
                    panic!("node A must be a group")
                };
                let shape = shape_bounds(node).expect("node shape");
                let label_group = doc
                    .descendants()
                    .find(|candidate| {
                        candidate.has_tag_name("g")
                            && candidate.attribute("data-merman-foreignobject") == Some("fallback")
                            && candidate
                                .attribute("class")
                                .is_some_and(|class| class.contains("nodeLabel"))
                            && normalized_text(*candidate)
                                == label
                                    .chars()
                                    .filter(|character| !character.is_whitespace())
                                    .collect::<String>()
                    })
                    .unwrap_or_else(|| panic!("rendered label not found for {label:?}"));
                let lines = label_group
                    .descendants()
                    .filter(|candidate| candidate.has_tag_name("text"))
                    .collect::<Vec<_>>();
                assert!(!lines.is_empty(), "node A must contain rendered text");
                for line in lines {
                    let line_text = line.text().unwrap_or_default();
                    let metrics = <GithubFlowchartText as merman::render::TextMeasurer>::measure(
                        &measurer, line_text, &style,
                    );
                    let x = number(line, "x") + view_box_x;
                    let y = number(line, "y") + view_box_y;
                    let text = resvg::usvg::tiny_skia_path::Rect::from_xywh(
                        (x - metrics.width / 2.) as f32,
                        (y - metrics.height / 2.) as f32,
                        metrics.width as f32,
                        metrics.height as f32,
                    )
                    .expect("measured text must have positive bounds");
                    assert!(
                        text.left() >= shape.left() - 1.
                            && text.right() <= shape.right() + 1.
                            && text.top() >= shape.top() - 1.
                            && text.bottom() <= shape.bottom() + 1.,
                        "label {label:?} must stay inside {open}{close}: text={text:?}, shape={shape:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn mermaid_svg_raster_size_tracks_the_display_scale_factor() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="40" viewBox="0 0 120 40"><rect width="120" height="40"/></svg>"#;
        let renderer = gpui::SvgRenderer::new(Arc::new(()));
        let one = renderer.render_single_frame(svg, 1.).unwrap();
        let two = renderer.render_single_frame(svg, 2.).unwrap();
        assert_eq!(two.size(0).width.0, one.size(0).width.0 * 2);
        assert_eq!(two.size(0).height.0, one.size(0).height.0 * 2);
        let zoomed = renderer
            .render_single_frame(svg, raster_scale(svg, 4.))
            .unwrap();
        assert_eq!(zoomed.size(0).width.0, one.size(0).width.0 * 4);
        let huge = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10000" height="20000" viewBox="0 0 10000 20000"><rect width="10000" height="20000"/></svg>"#;
        let bounded = renderer
            .render_single_frame(huge, raster_scale(huge, 16.))
            .unwrap();
        let size = bounded.size(0);
        assert!(size.width.0 <= 8192 && size.height.0 <= 8192);
        assert!(i64::from(size.width.0) * i64::from(size.height.0) <= 16_777_216);
    }

    #[gpui::test]
    fn expanded_tall_diagram_opens_at_readable_size_and_scrolls_to_its_end(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_, mut vcx) = crate::ui::app::test_window::harness(cx);
        vcx.simulate_resize(gpui::size(px(1000.), px(800.)));
        vcx.update(|window, cx| {
            super::super::markdown_preview::init(Default::default(), cx);
            let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="4000" viewBox="0 0 320 4000"><text x="20" y="40" font-size="16">First readable node</text><text x="20" y="3980" font-size="16">LAST_NODE</text></svg>"#;
            open_diagram(
                ImageSource::Image(std::sync::Arc::new(gpui::Image::from_bytes(
                    gpui::ImageFormat::Svg,
                    svg.to_vec(),
                ))),
                "tall diagram".into(),
                320. / 4000.,
                window,
                cx,
            );
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        let image = vcx.debug_bounds("diagram-image").unwrap();
        let viewport = vcx.debug_bounds("diagram-viewport").unwrap();
        assert!(
            image.size.height >= px(4000.),
            "opening a tall diagram must retain its readable size instead of fitting its height"
        );
        assert!(image.size.height > viewport.size.height * 4.);
        let drag_start = viewport.center();
        let drag_end = drag_start - point(px(0.), px(96.));
        vcx.simulate_event(gpui::MouseDownEvent {
            position: drag_start,
            button: MouseButton::Left,
            modifiers: gpui::Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        vcx.simulate_event(gpui::MouseMoveEvent {
            position: drag_end,
            pressed_button: Some(MouseButton::Left),
            modifiers: gpui::Modifiers::none(),
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert!(
            vcx.debug_bounds("diagram-image").unwrap().origin.y < image.origin.y,
            "dragging up must pan the expanded diagram toward its bottom"
        );
        vcx.simulate_event(gpui::MouseUpEvent {
            position: drag_end,
            button: MouseButton::Left,
            modifiers: gpui::Modifiers::none(),
            click_count: 1,
        });
        vcx.simulate_keystrokes("end");
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let last = vcx.debug_bounds("diagram-image").unwrap();
        let viewport = vcx.debug_bounds("diagram-viewport").unwrap();
        assert_eq!(last.size, image.size, "End preserves the reading size");
        assert!((last.bottom() - viewport.bottom()).abs() <= px(1.));
        vcx.simulate_keystrokes("home");
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let viewport = vcx.debug_bounds("diagram-viewport").unwrap();
        let thumb_start = point(viewport.right() - px(8.), viewport.top() + px(24.));
        let thumb_end = point(viewport.right() - px(8.), viewport.bottom() - px(20.));
        vcx.simulate_event(gpui::MouseDownEvent {
            position: thumb_start,
            button: MouseButton::Left,
            modifiers: gpui::Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        // The shared scrollbar throttles native thumb updates to 120 Hz.
        std::thread::sleep(std::time::Duration::from_millis(12));
        vcx.simulate_event(gpui::MouseMoveEvent {
            position: thumb_end,
            pressed_button: Some(MouseButton::Left),
            modifiers: gpui::Modifiers::none(),
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        vcx.simulate_event(gpui::MouseUpEvent {
            position: thumb_end,
            button: MouseButton::Left,
            modifiers: gpui::Modifiers::none(),
            click_count: 1,
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert!(
            (vcx.debug_bounds("diagram-image").unwrap().bottom()
                - vcx.debug_bounds("diagram-viewport").unwrap().bottom())
            .abs()
                <= px(1.),
            "dragging the actual scrollbar must reach the last node"
        );
        let fit = vcx.debug_bounds("diagram-fit").unwrap().center();
        vcx.simulate_click(fit, gpui::Modifiers::none());
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let fitted = vcx.debug_bounds("diagram-image").unwrap();
        let viewport = vcx.debug_bounds("diagram-viewport").unwrap();
        assert!(fitted.size.height <= viewport.size.height + px(1.));
        let reset = vcx.debug_bounds("diagram-reset").unwrap().center();
        vcx.simulate_click(reset, gpui::Modifiers::none());
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert_eq!(vcx.debug_bounds("diagram-image").unwrap().size, image.size);
        vcx.update(|_, cx| cx.global_mut::<crate::core::config::Config>().ui_font_size = 32.);
        vcx.simulate_resize(gpui::size(px(400.), px(300.)));
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert!(
            vcx.debug_bounds("diagram-viewport").unwrap().bottom() <= px(284.),
            "a large text scale in a short window must keep the viewer within the window"
        );
        assert!(
            vcx.debug_bounds("diagram-toolbar").unwrap().bottom() <= px(284.),
            "the toolbar must remain visible in a short window"
        );
        assert!(
            vcx.debug_bounds("diagram-toolbar").unwrap().bottom()
                <= vcx.debug_bounds("diagram-viewport").unwrap().top()
                && vcx.debug_bounds("diagram-toolbar").unwrap().right()
                    <= vcx.debug_bounds("diagram-viewport").unwrap().right(),
            "the toolbar row must sit above the short viewer, not over it"
        );
        assert!(
            vcx.debug_bounds("diagram-viewport").unwrap().size.width <= px(400.),
            "the expanded dialog must follow a narrowed window"
        );
    }

    #[gpui::test]
    fn expanded_keyboard_navigation_reaches_horizontal_overflow(cx: &mut gpui::TestAppContext) {
        let (_, mut vcx) = crate::ui::app::test_window::harness(cx);
        vcx.simulate_resize(gpui::size(px(1000.), px(800.)));
        vcx.update(|window, cx| {
            super::super::markdown_preview::init(Default::default(), cx);
            let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="900" viewBox="0 0 1600 900"><text x="20" y="40">LEFT</text><text x="1500" y="40">RIGHT</text></svg>"#;
            open_diagram(
                ImageSource::Image(std::sync::Arc::new(gpui::Image::from_bytes(
                    gpui::ImageFormat::Svg,
                    svg.to_vec(),
                ))),
                "wide diagram".into(),
                1600. / 900.,
                window,
                cx,
            );
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        let initial = vcx.debug_bounds("diagram-image").unwrap();
        let viewport = vcx.debug_bounds("diagram-viewport").unwrap();
        assert!(initial.size.width <= viewport.size.width);
        vcx.simulate_keystrokes("+");
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let zoomed = vcx.debug_bounds("diagram-image").unwrap();
        assert!(zoomed.size.width > viewport.size.width);
        vcx.simulate_keystrokes("right");
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let moved = vcx.debug_bounds("diagram-image").unwrap();
        assert!(
            moved.left() < zoomed.left(),
            "Right must reveal content on the right after zoom"
        );
        assert_eq!(moved.size, zoomed.size);
        vcx.simulate_keystrokes("left");
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert!(
            (vcx.debug_bounds("diagram-image").unwrap().left() - zoomed.left()).abs() <= px(1.)
        );
        for _ in 0..20 {
            vcx.simulate_keystrokes("right");
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        }
        assert!(
            (vcx.debug_bounds("diagram-image").unwrap().right() - viewport.right()).abs() <= px(1.)
        );
    }

    #[gpui::test]
    fn mermaid_unmodified_wheel_zooms_a_tall_diagram_at_the_pointer(cx: &mut gpui::TestAppContext) {
        let (_, mut vcx) = crate::ui::app::test_window::harness(cx);
        vcx.simulate_resize(gpui::size(px(1000.), px(800.)));
        vcx.update(|window, cx| {
            super::super::markdown_preview::init(Default::default(), cx);
            let source =
                "flowchart TD\nA[First] --> B[Second] --> C[Third] --> D[Fourth] --> E[Last]";
            let svg =
                render_mermaid(source, &crate::core::markdown_theme::builtin(), false).unwrap();
            open_diagram(
                ImageSource::Image(std::sync::Arc::new(gpui::Image::from_bytes(
                    gpui::ImageFormat::Svg,
                    svg.as_bytes().to_vec(),
                ))),
                source.into(),
                aspect_ratio(&svg),
                window,
                cx,
            );
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        vcx.simulate_keystrokes("+");
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let zoomed = vcx.debug_bounds("diagram-content").unwrap();
        let viewport = vcx.debug_bounds("diagram-viewport").unwrap();
        vcx.simulate_event(ScrollWheelEvent {
            position: viewport.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-120.))),
            modifiers: gpui::Modifiers::none(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let scrolled = vcx.debug_bounds("diagram-content").unwrap();
        assert!(
            scrolled.size.width < zoomed.size.width,
            "ordinary wheel down must zoom the expanded diagram out"
        );
        let before = (viewport.center().y - zoomed.top()) / zoomed.size.height;
        let after = (viewport.center().y - scrolled.top()) / scrolled.size.height;
        assert!(
            (before - after).abs() < 0.01,
            "wheel zoom must retain the pointer anchor"
        );
    }

    #[gpui::test]
    fn expanded_mermaid_keyboard_wheel_and_reset_repaint_the_diagram(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_, mut vcx) = crate::ui::app::test_window::harness(cx);
        vcx.simulate_resize(gpui::size(px(1000.), px(800.)));
        vcx.update(|window, cx| {
            super::super::markdown_preview::init(Default::default(), cx);
            let source = "graph TD; A-->B";
            let svg =
                render_mermaid(source, &crate::core::markdown_theme::builtin(), false).unwrap();
            let aspect = aspect_ratio(&svg);
            open_diagram(
                ImageSource::Image(std::sync::Arc::new(gpui::Image::from_bytes(
                    gpui::ImageFormat::Svg,
                    svg.into_bytes(),
                ))),
                source.into(),
                aspect,
                window,
                cx,
            );
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        let fitted = vcx.debug_bounds("diagram-content").unwrap();
        let viewport = vcx.debug_bounds("diagram-viewport").unwrap();
        assert!(vcx.debug_bounds("diagram-zoom-in").is_none());
        assert!(vcx.debug_bounds("diagram-pan-right").is_none());
        vcx.simulate_keystrokes("+");
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert!(
            (vcx.debug_bounds("diagram-content").unwrap().size.width - fitted.size.width * 1.25)
                .abs()
                <= px(1.)
        );
        vcx.simulate_event(ScrollWheelEvent {
            position: viewport.origin + point(px(32.), px(64.)),
            delta: ScrollDelta::Lines(point(0., 1.)),
            modifiers: gpui::Modifiers {
                control: true,
                ..gpui::Modifiers::none()
            },
            touch_phase: gpui::TouchPhase::Moved,
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert!(
            vcx.debug_bounds("diagram-content").unwrap().size.width > fitted.size.width * 1.25,
            "wheel must enlarge the expanded diagram"
        );
        for (delta, expected) in [(20_000., 8.), (-20_000., 0.25)] {
            vcx.simulate_event(ScrollWheelEvent {
                position: viewport.origin + point(px(32.), px(64.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(delta))),
                modifiers: gpui::Modifiers {
                    control: true,
                    ..gpui::Modifiers::none()
                },
                touch_phase: gpui::TouchPhase::Moved,
            });
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            assert!(
                (vcx.debug_bounds("diagram-content").unwrap().size.width
                    - fitted.size.width * expected)
                    .abs()
                    <= px(1.)
            );
        }
        // Resolve the current button after the dialog's opening animation.
        let reset = vcx.debug_bounds("diagram-reset").unwrap();
        vcx.simulate_click(reset.center(), gpui::Modifiers::none());
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let restored = vcx.debug_bounds("diagram-content").unwrap();
        assert_eq!(restored.size, fitted.size);
        // The dialog can still be animating; reset is relative to its viewport.
        assert_eq!(
            restored.origin,
            vcx.debug_bounds("diagram-viewport").unwrap().origin
                + point(
                    (vcx.debug_bounds("diagram-viewport").unwrap().size.width
                        - restored.size.width)
                        / 2.,
                    px(0.)
                )
        );
    }

    #[test]
    fn fitted_inline_diagrams_can_be_dragged() {
        let mut viewer = DiagramViewer::new("diagram.svg".into(), "".into(), 2., false);
        viewer.image_width = px(600.);
        viewer.height = px(400.);
        assert!(viewer.pan_by(point(px(32.), px(-32.))));
        assert_eq!(viewer.pan, point(px(32.), px(-32.)));
    }

    #[test]
    fn zoom_preserves_the_diagram_point_under_the_pointer() {
        let mut viewer = DiagramViewer::new("diagram.svg".into(), "".into(), 2., false);
        viewer.viewport =
            gpui::Bounds::new(point(px(40.), px(60.)), gpui::size(px(600.), px(400.)));
        viewer.image_width = px(480.);
        viewer.height = px(400.);
        let pointer = point(px(200.), px(150.));
        let before = (pointer - viewer.viewport.center() - viewer.pan) / viewer.zoom;
        viewer.zoom_at(1.25, pointer);
        let after = (pointer - viewer.viewport.center() - viewer.pan) / viewer.zoom;
        assert_eq!(before, after);
    }

    #[test]
    fn expanded_zoom_preserves_the_vertical_reading_anchor() {
        let mut viewer = DiagramViewer::new("diagram.svg".into(), "".into(), 0.4, true);
        viewer.viewport =
            gpui::Bounds::new(point(px(40.), px(60.)), gpui::size(px(600.), px(400.)));
        viewer.width = px(600.);
        viewer.image_width = px(480.);
        viewer.height = px(400.);
        viewer.scroll.set_offset(point(px(0.), px(-200.)));
        let pointer = point(px(340.), px(150.));
        let before = (pointer.y - viewer.viewport.top() - viewer.scroll.offset().y) / viewer.zoom;
        viewer.zoom_at(1.25, pointer);
        let after = (pointer.y - viewer.viewport.top() - viewer.scroll.offset().y) / viewer.zoom;
        assert_eq!(before, after);
    }

    #[test]
    fn builtin_mermaid_matches_live_github_canvas_padding() {
        let source = "flowchart LR\n  Choose[Choose reading theme] --> Palette[Resolve light or dark palette]\n  Palette --> Preview[Render Markdown preview]\n";
        let svg = render_mermaid(source, &crate::core::markdown_theme::builtin(), false).unwrap();
        let live: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/github-live-2026-10-06/mermaid-light.json"
        ))
        .unwrap();
        let viewbox = regex::Regex::new(r#"viewBox="([^"]+)""#).unwrap();
        let native: Vec<f32> = viewbox.captures(&svg).unwrap()[1]
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        let website: Vec<f32> = live["viewBox"]
            .as_str()
            .unwrap()
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        assert!(
            (native[3] - website[3]).abs() < 1.,
            "Mermaid canvas height differs from live GitHub: {native:?} vs {website:?}"
        );
    }

    #[test]
    fn diagram_canvas_matches_the_builtin_and_custom_background_contract() {
        let custom = crate::core::markdown_theme::parse(
            "schema_version: 2\nid: diagram-background-test\nname: Diagram background test\nlight:\n  background: '#f2e9df'\n  paper: '#fffaf3'\ndark:\n  background: '#172a3a'\n  paper: '#20384d'\n",
        )
        .unwrap();
        let builtin = crate::core::markdown_theme::builtin();
        let background = regex::Regex::new(r#"background-color:\s*([^;"<>]+)"#).unwrap();
        for (name, source) in [
            (
                "flowchart",
                "flowchart LR\n A[Start] -->|Continue| B[Finish]",
            ),
            (
                "sequence",
                "sequenceDiagram\n Alice->>Bob: Hello\n Bob-->>Alice: Reply",
            ),
            (
                "cluster",
                "flowchart LR\n subgraph Group\n A[Start] --> B[Finish]\n end",
            ),
        ] {
            for dark in [false, true] {
                for (theme, expected) in [
                    (builtin.as_ref(), "transparent".to_string()),
                    (&custom, color_to_hex(&custom.palette(dark).background)),
                ] {
                    let svg = render_mermaid(source, theme, dark).unwrap();
                    let root = &svg[..svg.find('>').unwrap()];
                    let actual = background.captures(root).map(|c| c[1].trim().to_string());
                    assert_eq!(
                        actual.as_deref(),
                        Some(expected.as_str()),
                        "{name}, theme={}, dark={dark}, root={root}",
                        theme.id
                    );
                }
            }
        }
    }

    #[test]
    fn diagram_labels_are_portable_to_the_native_svg_renderer() {
        let svg = render_mermaid(
            "flowchart LR\n A[Visible label] --> B[Another label]",
            &crate::core::markdown_theme::builtin(),
            false,
        )
        .unwrap();
        assert!(
            svg.contains("<text"),
            "native SVG rendering requires portable text for HTML node labels"
        );
        assert!(
            svg.contains("Visible label"),
            "node labels must remain in the SVG"
        );
    }

    #[test]
    fn color_to_hex_formats_opaque_and_translucent() {
        assert_eq!(color_to_hex(&Color([1.0, 0.0, 0.5, 1.0])), "#FF0080");
        assert_eq!(color_to_hex(&Color([0.0, 0.0, 0.0, 0.5])), "#00000080");
    }
}
