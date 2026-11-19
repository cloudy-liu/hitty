//! Theme-relative terminal colors.
//!
//! An RGB/indexed fill can outlive the theme under which it was chosen. In
//! legible-color mode, move opposite-polarity fills toward the current canvas
//! and resolve text against the resulting background. Known Codex surfaces
//! follow the current palette, with a shared input/history fill in light themes.
//! Literal mode bypasses this adapter entirely.

use std::cell::RefCell;
use std::collections::HashMap;

use alacritty_terminal::vte::ansi::Rgb;
use smallvec::SmallVec;

use crate::ui::presets::{contrast, mix};

/// Per-frame caches avoid repeating contrast searches for every character in
/// a run. They are discarded with the paint colors, including on theme changes.
pub(crate) struct AdaptiveColors {
    canvas: u32,
    codex_fills: SmallVec<[(u32, u32); 4]>,
    backgrounds: RefCell<HashMap<(u32, bool), u32>>,
    foregrounds: RefCell<HashMap<(u32, u32, bool), u32>>,
}

impl AdaptiveColors {
    pub(crate) fn new(canvas: u32, codex_backgrounds: &[Rgb]) -> Self {
        let codex_fills = if codex_backgrounds.is_empty() {
            SmallVec::new()
        } else {
            let native = codex_fills(canvas);
            let live = if light_background(canvas) {
                [native[0]; 2]
            } else {
                native
            };
            codex_backgrounds
                .iter()
                .flat_map(|&Rgb { r, g, b }| {
                    codex_fills(u32::from_be_bytes([0, r, g, b]))
                        .into_iter()
                        .zip(live)
                })
                // A fresh fill can coincide with an old fill for a different surface.
                .filter(|(old, _)| !native.contains(old))
                // Current fills take precedence over a coinciding old surface.
                // Include them so fresh light sessions also share one fill.
                .chain(native.into_iter().zip(live))
                .collect()
        };
        Self {
            canvas,
            codex_fills,
            backgrounds: RefCell::new(HashMap::new()),
            foregrounds: RefCell::new(HashMap::new()),
        }
    }

    pub(crate) fn background(&self, source: u32, truecolor: bool) -> u32 {
        *self
            .backgrounds
            .borrow_mut()
            .entry((source, truecolor))
            .or_insert_with(|| {
                let source = if truecolor {
                    self.codex_fill(source).unwrap_or(source)
                } else {
                    source
                };
                theme_background(source, self.canvas)
            })
    }

    fn codex_fill(&self, source: u32) -> Option<u32> {
        let mut matches = self.codex_fills.iter().filter(|&&(old, _)| old == source);
        let &(_, fill) = matches.next()?;
        matches.all(|&(_, other)| fill == other).then_some(fill)
    }

    pub(crate) fn foreground(&self, source: u32, background: u32, dim: bool) -> u32 {
        *self
            .foregrounds
            .borrow_mut()
            .entry((source, background, dim))
            .or_insert_with(|| {
                readable_ink(
                    source,
                    background,
                    if dim {
                        super::element::DIM_OPACITY
                    } else {
                        1.0
                    },
                )
            })
    }
}

/// Codex CLI caches its OSC 11 answer and derives the composer/plan and
/// submitted-prompt fills from it. Match those two fills against the actual
/// answer from this pane, then recompute them against the current background.
/// See `codex-rs/tui/src/{style,color}.rs` at `openai/codex` `rust-v0.169.0`.
fn codex_fills(background: u32) -> [u32; 2] {
    let [r, g, b] = [16, 8, 0].map(|shift| ((background >> shift) & 255) as f32);
    let (shade, amounts) = if 0.299 * r + 0.587 * g + 0.114 * b > 128.0 {
        (0.0, [0.04, 0.02])
    } else {
        (255.0, [0.12, 0.16])
    };
    amounts.map(|amount| {
        [r, g, b].into_iter().fold(0, |rgb, channel| {
            (rgb << 8) | (shade * amount + channel * (1.0 - amount)) as u32
        })
    })
}

fn light_background(rgb: u32) -> bool {
    contrast(rgb, 0x000000) > contrast(rgb, 0xffffff)
}

fn channels(rgb: u32) -> [f32; 3] {
    [16, 8, 0].map(|shift| ((rgb >> shift) & 255) as f32 / 255.0)
}

fn lightness(rgb: [f32; 3]) -> (f32, f32) {
    let min = rgb.into_iter().fold(1.0, f32::min);
    let max = rgb.into_iter().fold(0.0, f32::max);
    ((min + max) / 2.0, (max - min) / 2.0)
}

fn theme_background(source: u32, canvas: u32) -> u32 {
    let light = light_background(canvas);
    if light_background(source) == light {
        return source;
    }
    let rgb = channels(source);
    let (level, half_chroma) = lightness(rgb);
    let (canvas_level, _) = lightness(channels(canvas));
    // Soft fills stay within eight lightness points of the live canvas. Keep
    // their relative depth near black/white, rather than flattening every fill
    // to one color. The gamut bound preserves chroma: fully saturated colors
    // cannot be shifted as far as neutral/pastel surfaces.
    let depth = if light { level } else { 1.0 - level }.min(0.08);
    let target = if light {
        canvas_level - depth
    } else {
        canvas_level + depth
    }
    .clamp(half_chroma, 1.0 - half_chroma);
    let shift = target - level;
    rgb.into_iter().fold(0, |out, c| {
        (out << 8) | (((c + shift).clamp(0.0, 1.0) * 255.0).round() as u32)
    })
}

/// Preserve readable ink; otherwise move it toward black or white. Measure
/// the composited text, including SGR dim, against its actual painted fill.
fn readable_ink(fg: u32, bg: u32, opacity: f32) -> u32 {
    let visible_contrast = |ink| contrast(mix(bg, ink, opacity), bg);
    if visible_contrast(fg) >= 4.5 {
        return fg;
    }
    let toward = if visible_contrast(0xffffff) >= visible_contrast(0x000000) {
        0xffffff
    } else {
        0x000000
    };
    if visible_contrast(toward) < 4.5 {
        return toward;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..16 {
        let mid = (lo + hi) / 2.0;
        if visible_contrast(mix(fg, toward, mid)) >= 4.5 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    mix(fg, toward, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_soft_fills_follow_any_canvas_without_rgb_matching() {
        for canvas in [0x161b22, 0x2c2135, 0xfffcf1, 0xe6eef8] {
            for gray in 0u32..=255 {
                // Exercise every grey, and small red/green/blue tints, rather
                // than a list of application colors.
                for tint in [None, Some(0), Some(1), Some(2)] {
                    let source = (0..3).fold(0, |out, channel| {
                        (out << 8)
                            | if tint == Some(channel) {
                                gray.saturating_sub(6)
                            } else {
                                gray
                            }
                    });
                    let result = theme_background(source, canvas);
                    assert_eq!(light_background(result), light_background(canvas));
                    let (_, before) = lightness(channels(source));
                    let (_, after) = lightness(channels(result));
                    assert!(
                        (before - after).abs() < 0.005,
                        "chroma changed for {source:06x}"
                    );
                    if light_background(source) == light_background(canvas) {
                        assert_eq!(result, source);
                    }
                }
            }
        }
    }

    #[test]
    fn saturated_fills_stay_in_gamut_and_readable_ink_stays_literal() {
        for canvas in [0x20252c, 0xffffff] {
            for source in [0xff0000, 0x00ff00, 0x0000ff] {
                assert_eq!(theme_background(source, canvas), source);
            }
        }
        assert_eq!(readable_ink(0xaaddff, 0x20252c, 1.0), 0xaaddff);
    }
}
