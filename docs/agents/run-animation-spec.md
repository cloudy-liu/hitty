## Problem Statement

The agent's yellow working symbol is static, so users cannot distinguish ongoing work by motion when scanning the tab sidebar. The user approved a standalone preview that rotates the existing half-circle.

## Solution

Rotate the existing yellow half-circle clockwise while its displayed status is working. Keep its center, dimensions, palette, background disc and surrounding content stable. Match the approved preview: one revolution per second with twelve discrete angular steps.

## User Stories

1. As a user watching several agents, I want working symbols to rotate so I can identify ongoing turns at a glance.
2. As a user of the left sidebar, I want the current yellow half-circle and size preserved so the existing visual language remains familiar.
3. As a user of the top tab bar, I want the same working symbol animation so changing tab-bar position keeps the status readable.
4. As a user reading a branch, title or status tag, I want that content to remain still so motion does not interfere with reading.
5. As a user whose agent needs input or has finished, I want rotation to stop as soon as the displayed status changes.
6. As a user running many agents, I want them to share animation timing so adding agents does not multiply refresh frequency.
7. As a user filtering or collapsing the sidebar, I want hidden working symbols to stop requesting animation redraws.
8. As a user without visible working symbols, I want the application to retain its existing idle rendering behavior.
9. As a user returning to hidden content, I want working symbols to resume rotation without affecting agent lifecycle, notifications or unread state.
10. As a user typing in the terminal, I want the animation to avoid a display-rate rendering loop and preserve input responsiveness.

## Implementation Decisions

- Extend the shared tab-avatar status rendering used by the sidebar and top tab bar. Preserve the bundled working SVG geometry and state palette.
- Use one scheduler per window and a shared phase for all visible working symbols. Target twelve refreshes per second; do not use one task per badge or a repeating display-frame animation.
- Gate animation scheduling by actual rendered/painted working badges, including clipping and sidebar filtering/group collapse where the framework supports observing paint visibility.
- Preserve working indicators in visible unfocused windows; loss of focus alone does not mean the window is hidden.
- Stop renewing animation work when no working badge is visible. Resume through ordinary application rendering when one becomes visible again.
- Leave the static tray rasterization, agent status detection, unread behavior, notifications and daemon/CLI protocol unchanged.
- Reuse existing framework and platform dependencies. Avoid new packages or general-purpose animation configuration.
- Investigate framework support for hidden/minimized windows and reduced-motion preferences. Record any unsupported platform behavior explicitly rather than claim it is verified.

## Testing Decisions

- User-confirmed boundary: the existing GPUI application/window rendering entry with real agent status events and the existing virtual clock/render counter. The user instructed us to continue after the boundary was presented.
- Verify externally observable animation and rendering behavior instead of testing private timer fields or asserting implementation call counts.
- Follow existing GPUI application and agent lifecycle fixtures, with one failing behavior test followed by its minimal implementation in each TDD slice.
- Cover rotation, shared timing for multiple agents, immediate status transition to static indicators, filtering/group collapse, top/sidebar reuse and idle rendering after animation demand disappears.
- Compare baseline and implementation in a native Windows window with quiet fixture data. Measure render cadence and process CPU where available; distinguish headless regression evidence from native performance measurements.

## Out of Scope

- Animating agent logos, status text, sidebar rows or non-working statuses.
- Replacing the approved half-circle with an Orca-style gap ring.
- Changing status detection, restoring sessions, notification behavior or tray animation.
- Introducing a new animation library or broad settings redesign.

## Further Notes

The approved standalone preview is visual guidance, not evidence of GPUI performance. Orca uses browser compositor animation; the native implementation needs its own rendering measurements. A single implementation ticket can deliver this complete behavior and its validation without artificial layer-by-layer tickets.

Tracking: cloudy-liu/ctty7#141. Implementation ticket: cloudy-liu/ctty7#142.
