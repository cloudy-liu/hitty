# GitHub Markdown theme verification

Issue [#85](https://github.com/cloudy-liu/ctty7/issues/85) was completed on
2026-10-06 after [PR #88](https://github.com/cloudy-liu/ctty7/pull/88) and
[PR #91](https://github.com/cloudy-liu/ctty7/pull/91) merged.
PR #91's final [CI run](https://github.com/cloudy-liu/ctty7/actions/runs/37419240897)
passed all nine jobs and retains the macOS/Linux native screenshot artifacts.
The final component pin is `87c76bd70cd9cb988e6503503e71dd73abad03c5`.

The current contract is in the [specification](../specs/markdown-github-theme.md).
User configuration and format details are in
[Markdown reading themes](../customization/markdown-themes.mdx).

## Reference and acceptance target

GitHub colors and typography use the committed `github-markdown-css` 5.9.0
snapshots, with Primer control tokens and Octicons attribution in
`assets/markdown-themes/GITHUB-NOTICE`. Live layout references are committed in
`tests/fixtures/github-live-2026-10-05`; Rust, diff and Mermaid references are
in `tests/fixtures/github-live-2026-10-06`.

The built-in theme's outer and body backgrounds follow the application
background, as approved on 2026-10-05. Other GitHub element colors and
typography remain independent. Custom v2 packages retain their configured
background and paper colors. The content cap is 1012px with 32px padding
on each side, for a 1076px total reading-width cap.

## Automated coverage

- Theme tests cover complete light/dark styles, v2 validation, reserved and
  duplicate IDs, partial overrides, array replacement, explicit nulls, v1
  upgrade errors and package repair. Automated scale checks include 100%,
  150% and 200% in both modes.
- Configuration tests retain selected theme IDs and leave the saved file
  unchanged. Unavailable selections use GitHub until repaired or explicitly
  changed in Settings; valid custom packages follow the normal ID rules.
- Application tests cover theme reload, file-event isolation, preview and
  source round trips, unsaved edits, selection, undo, anchor navigation and
  reading-position restoration after reflow. Stale asynchronous results do
  not affect a different document; resources resolve through the owning Host.
- Layout tests cover heading code, block origins, tables and responsive README
  images. Percentage-width images retain their intrinsic aspect ratio.
- Production highlighter tests compare live GitHub Rust and diff samples in
  both modes, including foreground, background and weight. Mermaid tests cover
  a 150px reference flowchart, sequence margins and 12 background combinations.
- Real GPUI HTTP/Host image tests cover SVG loading, animated frames, XML
  prologs, embedded fonts, failures and allocation limits. Active-window tests
  check visible playback and repaint scheduling.
- The opt-in Windows test
  `native_github_cjk_uses_noto_at_body_and_heading_weights` passed with local
  Noto Sans SC installed. It checks actual shaping at body and heading weights;
  the application does not download or bundle that font.

Run the application checks with `cargo test --workspace --locked`.
Component checks run in the component checkout with
`cargo test -p gpui-component --lib --features tree-sitter-languages`.
Both application component crates use the same pinned Git revision.
Windows shell integration tests require Git's `bin` and `usr/bin` on PATH.
The opt-in CJK check runs with `-- --ignored` and requires Noto Sans SC.

## Native acceptance

The 2026-10-06 matrix inspected 24 native screenshots, 12 each on macOS and
Linux: light/dark, 1076px/500px and top/lists/code sections. The production
reading view uses real platform executors and font shaping. The capture script
`scripts/markdown-native-visual.py` checks viewport bounds, requested anchors
and nonempty images before uploading screenshots and logs.

Windows Computer Use checks ran at the existing 175% display scale, including
wide code and narrow dark lists. The
[acceptance sample](../examples/markdown-github-theme.md) and repository
READMEs were inspected for headings, inline code, links, tasks, five alerts,
overflowing tables, Rust/diff colors, image failures and unsaved source edits.

After capture, an obsolete theme name inside the sample's quoted diff string
was replaced with `old-theme`. This equal-length normalization preserves the
source offsets, token classes and captured styles; the remaining rendered
sample content is unchanged. See the
[live fixture README](../../tests/fixtures/github-live-2026-10-06/README.md)
for details.

Mermaid labels, transparent built-in canvases and custom backgrounds were
checked inline and expanded. Copy, zoom, pan, reset, close and focus recovery
worked. Diagram copy was verified by pasting into source and undoing without
saving. Application-theme changes preserved the reading state.

The README's remote title, badges and hero loaded. The supplied embedded bold
font appeared in the animated title; typing frames and a two-frame GIF changed
after source/reading round trips. The Chinese README retained image proportions
and approximately 44 logical pixels between its hero and the next heading,
including split-view restoration. Native Chinese shaping preferred Noto Sans SC
when available. Raw Windows screenshots remain local acceptance evidence.

## Limits

The extra Windows DPI matrix and application 200% visual checks were cancelled
by the user; automated scale coverage does not establish those native checks.
This acceptance does not establish identical browser/native glyph rasterization,
every language's syntax classification or all Mermaid horizontal geometry.
Real SSH/SFTP resource opening and near-limit document tail scrolling remain
unverified. See [project status](../maintenance/status.md) for release state
and remaining work. Source-editor visuals have a separate
[verification record](../specs/editor-github-light-verification.md).

SVG animation supports bounded parent-targeted SMIL with compatible looping
tracks. CSS/JavaScript animation, motion/transform animation, external fonts
and external nested resources are unsupported. Runtime limits and package
behavior are documented in the current specification.
