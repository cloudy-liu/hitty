//! Where the panes of a tab sit, and which one lies in a direction.
//!
//! One rule, shared by the window and the CLI. The window answers "which pane
//! does <kbd>Alt</kbd>+<kbd>→</kbd> focus"; the CLI answers "which pane does an
//! agent mean by *the one on the right*". Two answers to that question would
//! mean an agent talking to a pane the user was not pointing at.

use crate::core::machine::{Axis, PaneNode};

/// The share a split's first child may shrink or grow to. A stored ratio
/// outside it is drawn clamped, so it is laid out clamped here too.
pub const MIN_RATIO: f32 = 0.1;
pub const MAX_RATIO: f32 = 0.9;

/// A pane's place in its tab, in units of the tab: the tab is the unit square.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const UNIT: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    /// The two halves a split at `ratio` along `axis` cuts this area into.
    pub fn split(self, axis: Axis, ratio: f32) -> (Rect, Rect) {
        let r = ratio.clamp(MIN_RATIO, MAX_RATIO);
        match axis {
            Axis::Horizontal => {
                let aw = self.w * r;
                (
                    Rect { w: aw, ..self },
                    Rect {
                        x: self.x + aw,
                        w: self.w - aw,
                        ..self
                    },
                )
            }
            Axis::Vertical => {
                let ah = self.h * r;
                (
                    Rect { h: ah, ..self },
                    Rect {
                        y: self.y + ah,
                        h: self.h - ah,
                        ..self
                    },
                )
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    pub const ALL: [Direction; 4] = [
        Direction::Left,
        Direction::Right,
        Direction::Up,
        Direction::Down,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Direction::Left => "left",
            Direction::Right => "right",
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }
}

fn overlap_1d(a0: f32, alen: f32, b0: f32, blen: f32) -> f32 {
    ((a0 + alen).min(b0 + blen) - a0.max(b0)).max(0.0)
}

/// The pane `dir` of `rects[from]`: the nearest one that shares an edge span
/// with it, and of several equally near, the one sharing the most.
///
/// "Shares the most" is what makes the answer match what a person sees. Left of
/// a column split 80/20, *the pane on the right* is the tall one; calling that
/// ambiguous would be asking a question nobody looking at the window has.
pub fn neighbor(rects: &[Rect], from: usize, dir: Direction) -> Option<usize> {
    const EPS: f32 = 1e-4;
    let f = *rects.get(from)?;
    let mut best: Option<(usize, f32, f32)> = None;
    for (i, c) in rects.iter().enumerate() {
        if i == from {
            continue;
        }
        let (dist, overlap) = match dir {
            Direction::Left => (f.x - (c.x + c.w), overlap_1d(f.y, f.h, c.y, c.h)),
            Direction::Right => (c.x - (f.x + f.w), overlap_1d(f.y, f.h, c.y, c.h)),
            Direction::Up => (f.y - (c.y + c.h), overlap_1d(f.x, f.w, c.x, c.w)),
            Direction::Down => (c.y - (f.y + f.h), overlap_1d(f.x, f.w, c.x, c.w)),
        };
        if dist < -EPS || overlap <= EPS {
            continue;
        }
        let better = match best {
            None => true,
            Some((_, bd, bo)) => dist < bd - EPS || (dist <= bd + EPS && overlap > bo + EPS),
        };
        if better {
            best = Some((i, dist, overlap));
        }
    }
    best.map(|(i, _, _)| i)
}

/// Every pane of a tab with the area it occupies, in tree order.
pub fn leaf_rects(root: &PaneNode) -> Vec<(u64, Rect)> {
    fn collect(node: &PaneNode, area: Rect, out: &mut Vec<(u64, Rect)>) {
        match node {
            PaneNode::Leaf { pane } => out.push((*pane, area)),
            PaneNode::Split { axis, ratio, a, b } => {
                let (first, second) = area.split(*axis, *ratio);
                collect(a, first, out);
                collect(b, second, out);
            }
        }
    }
    let mut out = Vec::new();
    collect(root, Rect::UNIT, &mut out);
    out
}

/// The pane `dir` of `pane` in the tab rooted at `root`.
pub fn neighbor_pane(root: &PaneNode, pane: u64, dir: Direction) -> Option<u64> {
    let leaves = leaf_rects(root);
    let from = leaves.iter().position(|(id, _)| *id == pane)?;
    let rects: Vec<Rect> = leaves.iter().map(|(_, rect)| *rect).collect();
    neighbor(&rects, from, dir).map(|i| leaves[i].0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(pane: u64) -> Box<PaneNode> {
        Box::new(PaneNode::Leaf { pane })
    }

    fn split(axis: Axis, ratio: f32, a: Box<PaneNode>, b: Box<PaneNode>) -> Box<PaneNode> {
        Box::new(PaneNode::Split { axis, ratio, a, b })
    }

    #[test]
    fn a_side_by_side_split_has_a_left_and_a_right() {
        let root = split(Axis::Horizontal, 0.58, leaf(47), leaf(52));
        assert_eq!(neighbor_pane(&root, 47, Direction::Right), Some(52));
        assert_eq!(neighbor_pane(&root, 52, Direction::Left), Some(47));
        assert_eq!(neighbor_pane(&root, 47, Direction::Left), None);
        assert_eq!(neighbor_pane(&root, 47, Direction::Up), None);
        assert_eq!(neighbor_pane(&root, 99, Direction::Right), None);
    }

    #[test]
    fn of_two_panes_equally_near_the_one_sharing_more_of_the_edge_wins() {
        // 1 | 2
        //   | -
        //   | 3      2 takes 80% of the right column
        let root = split(
            Axis::Horizontal,
            0.5,
            leaf(1),
            split(Axis::Vertical, 0.8, leaf(2), leaf(3)),
        );
        assert_eq!(neighbor_pane(&root, 1, Direction::Right), Some(2));
        assert_eq!(neighbor_pane(&root, 2, Direction::Down), Some(3));
        assert_eq!(neighbor_pane(&root, 3, Direction::Left), Some(1));
    }

    #[test]
    fn the_nearest_pane_wins_over_a_farther_one_behind_it() {
        // 1 | 2 | 3
        let root = split(
            Axis::Horizontal,
            0.3,
            leaf(1),
            split(Axis::Horizontal, 0.5, leaf(2), leaf(3)),
        );
        assert_eq!(neighbor_pane(&root, 1, Direction::Right), Some(2));
        assert_eq!(neighbor_pane(&root, 3, Direction::Left), Some(2));
    }

    #[test]
    fn a_ratio_outside_the_drawable_range_is_laid_out_as_it_is_drawn() {
        let root = split(Axis::Horizontal, 0.99, leaf(1), leaf(2));
        let rects = leaf_rects(&root);
        assert_eq!(rects[0].1.w, MAX_RATIO);
        assert!((rects[1].1.x - MAX_RATIO).abs() < 1e-6);
    }
}
