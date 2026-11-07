# Diff rendering performance

Verified on Windows on 2026-10-15. The sidebar's red/green counts enter
`toggle_diff_overlay`, read the HEAD patch through `HostOps`, and render it through
`diff_file_list`. The native fixture calls that same list renderer with a real
Git snapshot and native text. Git IO is measured separately.

## Reproduction and diagnosis

Use a repository with 24 tracked empty files, then add 300 distinct lines to each
file without committing. Open its diff from the sidebar counts and scroll. All
7,200 retained lines are below the 8,000-line repository collapse threshold, and
each file is below the 400-change per-file threshold, so every card expands.

Before the fix, the renderer built, laid out and prepainted every expanded row,
including rows outside the viewport. Character selection also shaped the hidden
rows and collected their grapheme boundaries. Clipping the painted result did
not avoid that work.

The following diagnostic runs used native Windows text and the same optimized
binary. Each switch changed one variable; the temporary switches were removed
after diagnosis.

| Diagnostic | First CPU frame | Scroll median | Scroll P95 |
| --- | ---: | ---: | ---: |
| 24 files / 7,200 lines | 696.81 ms | 448.41 ms | 461.07 ms |
| 1 file / 300 lines | 42.64 ms | 17.12 ms | 17.74 ms |
| 24 files, width measurement bypassed | 697.83 ms | 443.96 ms | 457.94 ms |
| 24 files, character selection geometry bypassed | 579.29 ms | 342.37 ms | 348.95 ms |

This identifies whole-document row work as the main cost. Selection geometry
adds to it; width measurement alone does not explain the original regression.
Git reads took approximately 176–187 ms in these native diagnostic runs.
Raw diagnostic results: [default](diff-performance-data/diagnostic-default.json),
[single file](diff-performance-data/diagnostic-small.json),
[without width measurement](diff-performance-data/diagnostic-without-width.json),
[without selection geometry](diff-performance-data/diagnostic-without-selection.json).

The automated reproduction also failed before the fix:

```text
diff_many_files_frame_profile --ignored --nocapture --test-threads=1
opening the diff blocks for 3218.1 ms
```

That test uses GPUI's test text system and was run during compilation. Its
timings are a reproduction signal, not the native performance comparison.
After the fix, the same command passed in both modes: split first frame
28.71 ms and scroll P95 10.80 ms; unified first frame 17.01 ms and scroll P95
10.76 ms. This rerun also had an active native build, so use the separate native
comparison for performance claims.

## Native comparison

These are medians of three separate trial statistics. The scroll P95 is the
median of the three per-trial P95 values, not a pooled percentile. Each mode
used three interleaved before/after process pairs with no Cargo build running.

| Mode / statistic | Before | After | CPU time reduction |
| --- | ---: | ---: | ---: |
| Split first frame | 694.72 ms | 29.32 ms | 95.78% |
| Split scroll median | 445.53 ms | 8.77 ms | 98.03% |
| Split scroll P95 | 460.40 ms | 9.72 ms | 97.89% |
| Unified first frame | 927.52 ms | 29.72 ms | 96.80% |
| Unified scroll median | 626.86 ms | 9.38 ms | 98.50% |
| Unified scroll P95 | 647.19 ms | 10.12 ms | 98.44% |

Git IO is outside those figures. The three-trial median Git probe was
181.63 ms before / 195.09 ms after for split and
184.91 ms before / 198.86 ms after for unified. This patch does not optimize
Git IO; opening still waits for a fresh snapshot.

| Trial | Split first / median / P95, before → after (ms) | Unified first / median / P95, before → after (ms) |
| --- | --- | --- |
| 1 | 688.79 / 445.53 / 461.49 → 30.50 / 8.78 / 9.90 | 931.48 / 626.93 / 636.76 → 29.72 / 9.32 / 9.85 |
| 2 | 700.73 / 447.51 / 460.40 → 29.32 / 8.77 / 9.22 | 927.52 / 626.86 / 655.69 → 30.50 / 9.38 / 10.12 |
| 3 | 694.72 / 439.36 / 456.45 → 29.17 / 8.74 / 9.72 | 918.67 / 623.07 / 647.19 → 29.03 / 9.38 / 10.41 |

All 12 native runs reported 24 files, 7,200 retained lines, 35 scroll samples
and a minimum scroll offset of -2,400 pixels. The result files are in
[diff-performance-data](diff-performance-data/summary.json), alongside
[binary/input hashes and machine metadata](diff-performance-data/metadata.json).
Per-run JSON filenames use `before|after` + `split|unified` + trial number.

## Implementation

Each hunk reserves its complete width and fixed 19-pixel row heights. During
prepaint it intersects its bounds with the actual content mask, then builds
only the intersecting rows. File bodies outside the viewport also defer their
column construction and text-width measurement. Hunk header heights include
their text style and padding, so placeholders and rendered bodies agree.

Selection keeps the complete original source text and logical row indices.
Only visible geometry is collected. This preserves Unicode, tabs, selections
whose anchors scroll offscreen, and copying across hunks with split padding.
When an entire file body becomes hidden, its pointer-drag task stops while the
selected text remains available.
Horizontal scroll handles remain alive when their file body is outside the
viewport. Every hunk reserves the maximum width of its column, including when
a later hunk contains the longest line.

The Git request, parsing budgets, collapse rules, palette and existing scroll
ownership are unchanged.

## Measurement protocol

- AMD Ryzen 7 5800H, 8 cores / 16 threads, 13.9 GiB usable RAM;
  Windows 11 `10.0.22000`, Rust `1.97.1`, x86-64 MSVC.
- Requested native viewport: 1000 × 700 logical pixels; bundled Hack font,
  dark mode, default application scale.
- Fixture text uses UTF-8 with LF endings and a final newline. For zero-based
  file and row indices, each line is
  `file {file} line {row}: let value = calculate(input);`.
- Optimized dev build: `--locked`, `profile.dev.debug=0`,
  `profile.dev.incremental=false`, `profile.dev.opt-level=2`. Existing GPUI
  and component package optimization remains 3.
- Baseline: `90bc795c` plus the opt-in profiling entry point. The diagnostic
  switches are disabled for the baseline comparison.
- Each process opens an empty window, settles for two seconds, installs the
  ready snapshot, then measures the first GPUI draw. Git probe timing precedes
  window creation and is reported separately.
- Scroll input: 40 wheel events of 120 logical pixels, 20 down and 20 up;
  discard the first five samples. Every run asserts that the real vertical
  offset reaches below -1,000 pixels. The fixture reaches -2,400 pixels.
- CPU measurements include event dispatch and GPUI layout/prepaint/paint.
  They exclude GPU execution and presentation. They are not displayed FPS or
  end-to-end sidebar-click timings.

## Running the checks

The deterministic regressions cover visible-row shaping, the last file and
last row, offscreen selection anchors, horizontal offset retention, and short
hunks preceding a long hunk. Existing tests cover both views, partial selection,
backward Unicode selection, tabs, hunk padding, edge drag and long-line endings.

The complete Windows GUI suite passed with **1,728 passed, 0 failed, 7 ignored**.
The opt-in diff performance test then passed separately. `cargo fmt --all
--check`, `git diff --check` and the host-boundary check passed. The production
rendering changes are shared Rust/GPUI code; no platform-specific rendering
branch or dependency change was added.

```powershell
cargo test --locked --bin tty7-app diff_overlay -- --test-threads=1
cargo test --locked --bin tty7-app diff_many_files_frame_profile -- --ignored --nocapture --test-threads=1
```

The native profiler reuses the existing opt-in visual-test feature:

```powershell
cargo build --locked --features markdown-visual-tests --bin tty7-app `
  --config profile.dev.debug=0 --config profile.dev.incremental=false `
  --config profile.dev.opt-level=2
```

Pass an options JSON file containing `cwd`, `output`, and `unified` to the built
executable. `cwd` is the local fixture repository and `output` is an absolute
JSON result path. Use a separate configuration directory:

```powershell
& .\target\debug\tty7-app.exe --config-dir .tty7-dev-diff-profile `
  --diff-profile C:\path\to\options.json
```

These measurements cover the synthetic local fixture on Windows. macOS, Linux,
remote-host IO, GPU timing and the original user's live repository were not
measured.
