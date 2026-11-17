# Terminal startup: current optimization and acceptance

Status: the 500 ms goal is **not met**. Native project-sidebar and split entry
points are now measured. The current Cmder candidate reduces local native-entry
P50 by about 385?520 ms (23?29%), with complete shell features in the tested
scenarios. Keep the PR draft. These are dev-build, local-machine measurements.

## Production Hitty changes

Each daemon pane creation takes one fresh configuration snapshot instead of
reading and parsing `config.json` up to five times for directory selection,
shell selection, environment, Codex attribution and engine selection. The next
pane still reads updated settings. A valid requested directory returns before
probing fallback directories.

The GUI imports and ranks history on a background worker alongside shell
creation. It joins both results before constructing the terminal view. Entries,
counts, directories, metadata and rankings remain available at input handoff.
The measurements below hold Hitty constant and quantify the Cmder candidate;
they do not quantify the benefit of these two Hitty changes.

## Cmder diagnosis and retained candidate

Seven timestamp probes in an unmodified isolated `init.bat`, over three starts,
put its own initialization at about 1.22?1.25 seconds. Roughly 620?650 ms was in
Git detection/configuration, including its PATH updates; another 320?330 ms was
in subsequent PATH/profile-directory processing. The Git stage is not a claim
that `git.exe` alone takes that long. Timestamp resolution is about 10 ms, and
this instrumented run is separate from the final comparisons.

The retained candidate makes four changes:

1. Expand no-argument base/logging library includes into the same variable
   assignments. Each call still refreshes variables and logging flags. This
   avoids repeatedly entering three batch files for every Git/PATH helper.
2. Call same-file Git helpers with `call :...` instead of re-entering their
   library and re-running its prelude.
3. Detect the shell with CMD string operations instead of three pipelines.
4. Retain the earlier guard that skips directory-enumeration shells for empty
   `profile.d` directories.

Git selection, PATH placement, alias migration, user scripts, Clink and prompt
behavior stay on their existing paths. The generator requires exact supported
patch blocks and validates the complete no-argument base/logging prelude before
inlining it. Additional custom initialization is rejected rather than omitted.
All installed Cmder vendor source hashes still match the copied baseline.

`prepare-cmder-startup.py` copies runtime/user scripts and licenses into baseline
and candidate fixtures. Git binaries are reused through directory junctions;
pending Git post-install scripts are rejected. These are isolated fixtures, not
an installed replacement. The real Cmder and Hitty configuration are unchanged.

## Current measured results ? 2026-11-17

The direct daemon/shell comparison uses 10 alternating-order pairs, one start
per process, with the same Hitty engine and bundled ConPTY for both copies:

| Cmder variant | Starts | Prompt P50 | Prompt P95 / max |
| --- | ---: | ---: | ---: |
| Installed-source copy | 10 | 1678.55 ms | 1941.64 ms |
| Current candidate | 10 | 1291.75 ms | 1473.99 ms |

The P50 difference is 386.80 ms (23.0%); median paired savings are 388.01 ms.
Every pair improved. All 20 starts passed alias Tab completion, arithmetic
execution and native history replay. Runs took place without builds/tests in
parallel. The earlier empty-profile-only trial saved about 43 ms; its historical
results remain in [the 2026-11-16 record](terminal-startup-evidence.json).

The native application fixture dispatches mouse events through the real project
sidebar ?+? button and the production split action. It uses real daemon panes,
profiles and shell integration. Each entry/directory/variant has 10 samples:

| Directory / entry | Original P50 | Candidate P50 | P50 saved | Original P95 / max | Candidate P95 / max |
| --- | ---: | ---: | ---: | ---: | ---: |
| Git / sidebar ?+? | 1797.55 ms | 1277.27 ms | 520.28 ms (28.9%) | 1869.86 ms | 1387.59 ms |
| Git / split | 1826.03 ms | 1356.23 ms | 469.80 ms (25.7%) | 1882.36 ms | 1504.39 ms |
| Non-Git / sidebar ?+? | 1660.37 ms | 1275.49 ms | 384.88 ms (23.2%) | 1794.59 ms | 1426.69 ms |
| Non-Git / split | 1708.97 ms | 1293.54 ms | 415.43 ms (24.3%) | 1890.23 ms | 1332.93 ms |

The native comparison holds the Hitty binary constant; these differences also
measure the Cmder candidate. Variants run in blocks, in the order original Git,
candidate Git, candidate non-Git, original non-Git. This is a small local sample,
not an alternating-pair GUI benchmark or company-machine acceptance.

**All 80 additional panes passed** completion, command execution, history replay,
Ctrl+C cancellation, empty Enter and prompt refresh. Replay requires new output,
so an old arithmetic result cannot satisfy a later command check. Each new pane
receives its first command immediately after prompt observation. The initial
fixture frame/repository detection settles before warmup and timed samples.
Cold fixture setup is recorded separately and excluded from the table.

The timed endpoint is prompt observation followed by GPUI draw return. It is not
display scanout and excludes OS mouse delivery. The four 500 ms gates correctly
failed after saving all measurements; these are performance failures with zero
functional failures in the final native dataset.

Exact source/binary/harness hashes, phase probes, all timings, failures and
limitations are in [the current evidence](terminal-startup-native-evidence.json).
Windows 11 / Ryzen 7 5800H; dev profile with existing dependency optimizations;
1100?700 logical window at 175% scale; Clink 1.2.46.69fc92; bundled ConPTY
1.24.260710001. Dependencies and pinned revisions were not changed.

## Reproduce on Windows

Use an idle machine and a new output directory for each setup. Python 3.11+ is
required for the native wrapper. Build and finish checks before measuring:

```powershell
cargo build --locked --features markdown-visual-tests --bin hitty-app
python scripts/measure-terminal-startup-native.py target/debug/hitty-app.exe D:/tools/cmder_full/cmder .scratch/native-original --samples 10 --budget-ms 500 --variant baseline
python scripts/measure-terminal-startup-native.py target/debug/hitty-app.exe D:/tools/cmder_full/cmder .scratch/native-candidate --samples 10 --budget-ms 500 --variant optimized
```

Use `--cwd` with a Git directory and an unrelated non-Git directory. A directory
under the repository is still a Git directory. The wrapper creates marked,
isolated Hitty profiles and cleans up only their daemons, including on failure.
The normal bundled `conpty.dll` and `OpenConsole.exe` must be beside the app.
Each run saves `measurements.json`, `summary.json`, `native.log`, options and
source hashes. Omit `--budget-ms` for functional/timing collection without a
performance threshold. A missing asset, input timeout or incomplete sample set
fails the run; failed samples are not silently dropped.

Direct shell diagnosis and behavior comparisons remain available:

```powershell
cargo build --locked -p hitty-core --example measure-pane-startup --no-default-features
Copy-Item target/debug/conpty.dll target/debug/examples/conpty.dll
Copy-Item target/debug/OpenConsole.exe target/debug/examples/OpenConsole.exe
python scripts/prepare-cmder-startup.py D:/tools/cmder_full/cmder .scratch/cmder-current --samples 10 --budget-ms 500
python scripts/verify-cmder-startup.py .scratch/cmder-current
target/debug/examples/measure-pane-startup.exe .scratch/cmder-current/baseline/options.json
target/debug/examples/measure-pane-startup.exe .scratch/cmder-current/optimized/options.json
```

## Validation and remaining acceptance

- `cargo test --locked --workspace --features updater`: 3341 passed, 14 ignored,
  zero failures. Installed-runtime coverage is the explicit fixture data above;
  optional installed-Cmder unit tests can skip internally.
- `cargo check --locked --workspace --all-targets --features updater`, native
  fixture build, Rust formatting and Python syntax: passed.
- 95 baseline/candidate behavior comparisons: PATH placement/filtering, fast and
  custom DIR modes, shell-path detection, logging/library variables, Git version
  ordering, legacy aliases and populated/empty profile directories: passed.
- A custom no-argument library initialization was correctly rejected.
- No production visual layout change; no image-based visual acceptance claimed.

An executable-directory wildcard fast path became slower in populated tool
folders and was discarded. A temporary-file probe saved only about 45 ms and was
also discarded. The final candidate keeps the original PATH directory/search
implementation. Exploratory native harness errors and an abandoned Hitty
before/after trial are listed in the evidence exclusions, not passing samples.

Further Cmder initialization work, a release-build comparison and acceptance on
the company machine remain. That machine?s custom synchronous
`git branch --show-current` prompt is unavailable locally. The target remains
less than 500 ms with full features; current results are still around 1.3 seconds.
