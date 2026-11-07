# GitHub Light source editor verification

Issue [#89](https://github.com/cloudy-liu/ctty7/issues/89) was completed when
[PR #90](https://github.com/cloudy-liu/ctty7/pull/90) merged on 2026-10-06.
All nine [CI checks](https://github.com/cloudy-liu/ctty7/actions/runs/37444566340)
passed on `73e99d52`. The current source appearance and typography contract
is in the [specification](editor-github-light.md).

## Result

Every resolved light application appearance uses GitHub Light with opaque
white source and gutter backgrounds. Dark appearances retain Atom One Dark.
The application controls system following, custom themes, previews and
cancellation. Settings name the active palette in English, Chinese and
Japanese; source typography remains independently configured.

GitHub's default file-viewer prettylights syntax tokens were captured on
2026-10-06, including gray comments (`#59636E`) and purple functions (`#6639BA`).
The pinned source records its published stylesheet URL, capture date and
checksum. Selection uses `#0969DA33`. Editable controls use CodeMirror source
tokens where the file viewer has no equivalent.

![Source palette comparison](images/editor-github-light-palettes.png)

This is a palette illustration from bundled values, not a native screenshot.

## Automated verification

- All 18 final editor-focused tests passed, covering source/control colors,
  settings, typography submission and bounds, search contrast, previews,
  configuration reload, editing state, language queries and focus restoration.
- Configuration regressions verify legacy and malformed `editor_theme` values
  are ignored on load and omitted on save while valid preferences survive.
- Production highlighting checks 220 dark TextMate expectations and 93 GitHub
  Light expectations across 21 languages, plus all 70 actual file-viewer color
  ranges in the reference Rust function. References are checked against source.
- Both palettes share 157 capture names. Light subroles inherit existing dark
  parent styles; all 38 private queries compile without changing shared
  language registrations or embedded-language behavior.
- Offline generation reproduces both palettes. Dark generated assets, pinned
  source, license and reference corpus retain their authored values.
- The locked Windows workspace suite and build passed, as did formatting,
  whitespace and the 98-file host-boundary check. The later combined startup
  acceptance passed 2,985 tests with eight ignored, recorded in
  [local terminal startup verification](../testing/local-terminal-startup.md).

```powershell
python scripts/sync-editor-themes.py
cargo test --locked -j 2 --features updater --bin tty7-app editor_
cargo test --locked -j 2 --workspace --features updater
cargo build --locked -j 2 --workspace --features updater
cargo fmt --all --check
git diff --check
bash .github/scripts/check-host-boundary.sh
```

Windows CLI integration tests require Git's `usr/bin` on PATH for their `sh`
subprocesses. Headless GPUI checks cover state and focus behavior; they do not
replace native screenshot acceptance.

## Windows native acceptance

Computer Use exercised the rebuilt application at the existing 175% display
scale on 2026-10-06. Source and gutter stayed white under a custom light
gradient, wallpaper and transparency; dark mode used Atom One Dark. Unsaved
text, cursor, scroll and undo survived theme and fill/dock changes. Searching
for `event` found 30 matches, and advancing moved the distinct current match.

![Native GitHub Light source editor](images/editor-github-light-native.png)

![Native dark source search](images/editor-dark-search-native.png)

The docked source footer kept a long path on one line with controls and the
cursor label visible. Opening and dismissing the theme palette restored source
focus: Right moved line 13, column 9 to column 10 without another click.

## Verification limits

Native macOS/Linux source-editor visuals were not checked. Their Markdown
screenshots are recorded separately in
[Markdown verification](../testing/markdown-github-theme.md).
Additional Windows DPI and application 200% visual checks are outside the
user-approved acceptance scope.

Tree-sitter maps existing syntax roles to website colors rather than reproducing
every browser or TextMate grammar. CMake, C#, GraphQL, Protocol Buffers and Swift
retain readable fallback text because their bundled queries are empty.
Semantic tokens and bracket-pair coloring remain outside this change.
