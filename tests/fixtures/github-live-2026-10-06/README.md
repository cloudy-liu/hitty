# Live GitHub follow-up captures

Captured on 2026-10-06 from the production GitHub rendering of
[`docs/examples/markdown-github-theme.md`](https://github.com/cloudy-liu/ctty7/blob/main/docs/examples/markdown-github-theme.md).
These observations are independent of the application's theme constants.

- `code-light.json` and `code-dark.json` record rendered Rust and diff
  tokens, their source offsets, computed foreground/background and font weight.
  Each file includes the source URL and capture timestamp.
- `mermaid-light.json` records the flowchart iframe's SVG viewBox, nodes and
  computed styles. Its URL identifies the GitHub Mermaid viewer. The sample
  source is the unchanged flowchart in the acceptance document.

The token regression compares every non-whitespace sample byte in both modes.
The Mermaid regression checks canvas height, which catches the extra label
line and missing canvas padding. It does not assert identical horizontal
geometry, fonts, or browser/native pixels.

The obsolete theme-name string in the diff samples was normalized to
`old-theme` after capture. This is an equal-length replacement inside a quoted
string; source offsets, token boundaries and captured styles are unchanged.
The acceptance document uses the same normalized source.
