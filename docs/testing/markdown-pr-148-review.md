# PR #148 review

Reviewed on 2026-10-15 in the order requested: Standards and Spec independently
in parallel, then ponytail-review, then verification of the findings.

The fixed comparison was:

```sh
git diff 46274f4aa6f18f099fd7f9cc163248ab6c795ab5...68b1082469603b81855edd78e297d394b41306a8
git log 46274f4aa6f18f099fd7f9cc163248ab6c795ab5..68b1082469603b81855edd78e297d394b41306a8 --oneline
```

The review covered all application changes, tests, profiling code, evidence,
and the component dependency diff from `e35384ce` to `1cdb30aa`. The subsequent
reset-key fix and regression received a separate review.

Sources: the repository's supplied AGENTS instructions, README development
checks, [issue-tracker rules](../agents/issue-tracker.md), the Fowler smell
baseline, [navigation specification](../specs/markdown-navigation-performance.md),
and [documented controls](../mermaid-support.md).

## Standards

No hard documented-standard violations were found.

- P3, possible Repeated Switches, a judgment call. In
  `src/ui/markdown_preview.rs`, `on_key_down` decodes shortcuts separately for
  virtualized and ordinary documents. For example,
  `"pageup" => list.scroll_by(-page)` and `"pageup" => offset.y + page`
  express the same navigation intent. Both branches also repeat platform
  document-edge shortcuts. Future binding changes must update both branches.
  Consider decoding navigation once before applying it to the active scroll
  owner.

No other actionable smell was found in the application or component diff.

## Spec

- P2, `0` failed to reset an inline diagram dragged at its initial zoom.
  The specification requires "Reset restores the initial reading size and
  position" and usable keyboard navigation. The control documentation names
  `0` as reset. In `src/ui/markdown_mermaid.rs`, the guard
  `"0" | "home" if self.expanded || self.zoom != 1.` rejected the key when
  an inline diagram still had zoom 1 but a nonzero pan.

No other correctness or scope issue was identified in the full diff.

The final manual compact-window, wide/sequence and source-edit checks remain
incomplete after the user stopped Computer Use. This is an acceptance evidence
gap, not a second identified code defect. The
[native verification record](github-mermaid-reading.md) preserves that limit.

## Ponytail review

Lean already. Ship.

## Review of the findings

- Accepted the P2 after reproducing it with an actual `0` key event in the
  cached reading view. Made `0` reset unconditionally. Expanded Home still
  takes its earlier match arm, retaining zoom and horizontal position while
  scrolling to the top. The Spec reviewer confirmed the fix resolves the P2.
- Retained the P3 as non-blocking maintenance advice. The two scroll owners
  require different operations, and their current mappings agree. Introducing
  a navigation type solely for this fix would add a refactor without repairing
  another observed behavior. Revisit the duplication when changing bindings.
- No separate dependency, abstraction or test deletion was justified by the
  complexity pass. The virtual-list, row-layout and selection-geometry code
  supports requirements in the specification; the assertions check visible
  movement and guard against timing an unchanged frame.

## Regression evidence

The existing `inline_mermaid_controls_zoom_pan_reset_and_copy` test now drags
the fitted inline diagram, presses `0`, draws, and checks the rendered transform.

```text
Before: 0 passed, 1 failed
0 must reset a fitted inline diagram after dragging
actual:   (1.0, Point { x: -54px, y: 10px })
expected: (1.0, Point { x:   0px, y:  0px })

After: Mermaid selection, 16 passed, 0 failed
```

Full workspace after the reset-key fix: **3,057 passed, 0 failed, 11 ignored**.
Formatting and host-boundary checks also pass. The local test build used
debug=0, incremental=false and dev opt-level=0, with the existing per-package
optimization overrides. Git for Windows usr/bin was on PATH.

```sh
cargo test --locked --bin tty7-app mermaid --features markdown-visual-tests -- --test-threads=1
cargo test --locked --workspace --features markdown-visual-tests -- --test-threads=1
```

The earlier native performance measurements and packaged executable are from
before this keyboard-only follow-up. They were not rerun or rebuilt for the
review; no new native manual acceptance is claimed.

Standards: one P3 advisory, no hard violation. Spec: one P2, fixed and re-reviewed.
