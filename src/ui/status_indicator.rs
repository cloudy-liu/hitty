//! The mark a tab avatar wears in its corner while an agent runs in the pane.
//!
//! Herdr's five states and its "distinct symbols" style: blocked `×`, done
//! `✓`, idle `○` and unknown `·` as herdr draws them, and working as a turning
//! comet — a solid head with a tail fading behind it — where herdr has `◐`.
//! A half-filled disc keeps a full circle's outline at every angle, so turning
//! it read as half done rather than busy. The old badge was a nine-pixel dot
//! that told working from done by hue alone and drew nothing at all for idle,
//! so an idle agent and one whose status tty7 could not read looked the same.
//! Every state is a different shape here as well as a different colour.

use crate::core::cli_agent::AgentStatus;
use crate::ui::i18n::{L10nKey, t};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusIndicator {
    /// Stopped on a question or a permission prompt — the one worth acting on.
    Blocked,
    Working,
    /// A finished turn nobody has looked at yet.
    Done,
    Idle,
    /// An agent tty7 recognised but has no status for: no hooks installed and
    /// no screen manifest, or neither has reported yet.
    Unknown,
}

impl StatusIndicator {
    pub const ALL: [StatusIndicator; 5] = [
        StatusIndicator::Blocked,
        StatusIndicator::Working,
        StatusIndicator::Done,
        StatusIndicator::Idle,
        StatusIndicator::Unknown,
    ];

    /// What an agent pane shows, given its status (hooks and screen together,
    /// `None` when neither has said anything) and whether its last result is
    /// unread.
    ///
    /// Herdr keeps a finished turn on "done" only until you look at it, then
    /// calls the agent idle again — so a read `Done` is `Idle` here.
    pub fn of_agent(status: Option<AgentStatus>, unread: bool) -> StatusIndicator {
        match status {
            None => StatusIndicator::Unknown,
            Some(AgentStatus::Done) if !unread => StatusIndicator::Idle,
            Some(status) => StatusIndicator::of_status(status),
        }
    }

    /// The indicator for a bare status, where nothing says whether a finished
    /// turn has been read (the tray's agent list).
    pub fn of_status(status: AgentStatus) -> StatusIndicator {
        match status {
            AgentStatus::Waiting => StatusIndicator::Blocked,
            AgentStatus::Working => StatusIndicator::Working,
            AgentStatus::Done => StatusIndicator::Done,
            AgentStatus::Idle => StatusIndicator::Idle,
        }
    }

    pub fn icon_path(self) -> &'static str {
        match self {
            StatusIndicator::Blocked => "icons/status/blocked.svg",
            StatusIndicator::Working => "icons/status/working.svg",
            StatusIndicator::Done => "icons/status/done.svg",
            StatusIndicator::Idle => "icons/status/idle.svg",
            StatusIndicator::Unknown => "icons/status/unknown.svg",
        }
    }

    /// Herdr's colours: red, yellow, teal, green and overlay0 from Catppuccin
    /// Mocha on a dark surface and Catppuccin Latte on a light one — the pair
    /// herdr ships as its default dark and light themes.
    pub fn rgb(self, dark: bool) -> u32 {
        match (self, dark) {
            (StatusIndicator::Blocked, true) => 0xF38BA8,
            (StatusIndicator::Working, true) => 0xF9E2AF,
            (StatusIndicator::Done, true) => 0x94E2D5,
            (StatusIndicator::Idle, true) => 0xA6E3A1,
            (StatusIndicator::Unknown, true) => 0x6C7086,
            (StatusIndicator::Blocked, false) => 0xD20F39,
            (StatusIndicator::Working, false) => 0xDF8E1D,
            (StatusIndicator::Done, false) => 0x179299,
            (StatusIndicator::Idle, false) => 0x40A02B,
            (StatusIndicator::Unknown, false) => 0x9CA0B0,
        }
    }

    /// The colour to draw this state in over `surface`: herdr's dark palette
    /// on a dark surface, its light one on a light surface.
    pub fn color(self, surface: gpui::Hsla) -> gpui::Rgba {
        gpui::rgb(self.rgb(crate::ui::presets::surface_is_dark(surface)))
    }

    /// Catppuccin Mocha's base, the surface herdr's dark colours are drawn
    /// for, for a mark that has to bring its own.
    pub const DARK_SURFACE: u32 = 0x1E1E2E;

    /// The short lowercase tag before a sidebar row's branch or working directory.
    /// Missing status has no text; the tab avatar still carries its neutral mark.
    pub fn sidebar_tag(self) -> Option<&'static str> {
        Some(match self {
            StatusIndicator::Blocked => "input",
            StatusIndicator::Working => "run",
            StatusIndicator::Done => "done",
            StatusIndicator::Idle => "idle",
            StatusIndicator::Unknown => return None,
        })
    }

    /// The words behind the symbol, for a tooltip.
    pub fn label(self) -> &'static str {
        t(match self {
            StatusIndicator::Blocked => L10nKey::AgentStatusWaiting,
            StatusIndicator::Working => L10nKey::AgentStatusWorking,
            StatusIndicator::Done => L10nKey::AgentStatusDone,
            StatusIndicator::Idle => L10nKey::AgentStatusIdle,
            StatusIndicator::Unknown => L10nKey::AgentStatusUnknown,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_turn_reads_done_only_until_it_is_seen() {
        assert_eq!(
            StatusIndicator::of_agent(Some(AgentStatus::Done), true),
            StatusIndicator::Done
        );
        assert_eq!(
            StatusIndicator::of_agent(Some(AgentStatus::Done), false),
            StatusIndicator::Idle
        );
    }

    #[test]
    fn an_agent_with_no_reported_status_is_unknown_not_idle() {
        assert_eq!(
            StatusIndicator::of_agent(None, false),
            StatusIndicator::Unknown
        );
        assert_eq!(
            StatusIndicator::of_agent(Some(AgentStatus::Idle), false),
            StatusIndicator::Idle
        );
    }

    #[test]
    fn waiting_on_the_user_is_herdrs_blocked() {
        for unread in [false, true] {
            assert_eq!(
                StatusIndicator::of_agent(Some(AgentStatus::Waiting), unread),
                StatusIndicator::Blocked
            );
            assert_eq!(
                StatusIndicator::of_agent(Some(AgentStatus::Working), unread),
                StatusIndicator::Working
            );
        }
    }

    #[test]
    fn every_state_is_its_own_shape_colour_and_words() {
        crate::ui::i18n::set_locale("en");
        for dark in [true, false] {
            let colours: std::collections::HashSet<u32> =
                StatusIndicator::ALL.iter().map(|s| s.rgb(dark)).collect();
            assert_eq!(colours.len(), StatusIndicator::ALL.len());
        }
        let icons: std::collections::HashSet<&str> =
            StatusIndicator::ALL.iter().map(|s| s.icon_path()).collect();
        assert_eq!(icons.len(), StatusIndicator::ALL.len());
        let labels: std::collections::HashSet<&str> =
            StatusIndicator::ALL.iter().map(|s| s.label()).collect();
        assert_eq!(labels.len(), StatusIndicator::ALL.len());
    }

    /// The badge turns clockwise, so the head has to lead at twelve o'clock
    /// with the tail fading anticlockwise behind it and nothing ahead of it.
    /// A closed outline turning in place — the old half-filled disc — keeps
    /// the same silhouette at every angle and reads as half done, not as busy.
    #[test]
    fn the_working_mark_is_a_comet_with_an_open_path_ahead_of_its_head() {
        use gpui::AssetSource as _;
        let svg = crate::ui::assets::Assets
            .load(StatusIndicator::Working.icon_path())
            .unwrap()
            .unwrap();
        let tree = resvg::usvg::Tree::from_data(&svg, &Default::default()).unwrap();
        const PX: u32 = 160;
        let scale = PX as f32 / tree.size().width();
        let mut pixmap = resvg::tiny_skia::Pixmap::new(PX, PX).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        // Ink on the track's centre line, `deg` clockwise from twelve o'clock.
        let alpha = |deg: f32| {
            let (sin, cos) = deg.to_radians().sin_cos();
            let (x, y) = (8. + 5.6 * sin, 8. - 5.6 * cos);
            pixmap
                .pixel((x * scale) as u32, (y * scale) as u32)
                .unwrap()
                .alpha()
        };

        assert!(alpha(-6.) > 240, "the head is not solid: {}", alpha(-6.));
        for ahead in [35., 55., 75.] {
            assert_eq!(alpha(ahead), 0, "ink {ahead}° ahead of the head");
        }
        let tail = [-15., -75., -135., -195., -255.].map(alpha);
        assert!(
            tail.windows(2).all(|w| w[0] > w[1]),
            "the tail does not fade away from the head: {tail:?}"
        );
        // At nine or ten pixels a tail that fades out early leaves only a
        // speck at the head, and the turning stops being visible.
        assert!(
            tail[4] >= 40,
            "the end of the tail is too faint to see: {tail:?}"
        );
    }

    #[test]
    fn the_sidebar_omits_unknown_text_but_shows_short_lowercase_tags() {
        let tags: Vec<_> = StatusIndicator::ALL
            .iter()
            .map(|s| s.sidebar_tag())
            .collect();
        assert_eq!(
            tags,
            [Some("input"), Some("run"), Some("done"), Some("idle"), None]
        );
    }
}
