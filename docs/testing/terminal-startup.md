# Terminal startup: preserved features and current performance limit

Status: partial optimization; the 500 ms goal is **not met**. The project-group
sidebar button's click-to-editable-prompt time remains unverified. Keep the PR
draft until that acceptance is completed or the narrower scope is accepted.

## Production changes

Each daemon pane creation now takes one fresh configuration snapshot instead
of reading and parsing `config.json` up to five times for directory selection,
shell selection, environment, Codex attribution and engine selection. The next
pane still reads updated settings. A valid requested directory returns before
probing fallback directories.

The GUI starts history import and ranking on a background worker alongside
shell creation. It joins both results before constructing the terminal view;
history entries, counts, directories, metadata and rankings are present when
input becomes available. This overlaps independent work and removes disk reads
and sorting from that UI-thread constructor. No persistent history cache or
delayed history hydration is introduced.

## Isolated Cmder experiment

`prepare-cmder-startup.py` copies the installed shell runtime and user scripts
into baseline and candidate fixtures. Git binaries are reused through directory
junctions; a pending Git post-install script is rejected. The installed Cmder
files are not patched. Third-party files and licenses stay in the local fixture,
outside the tracked source tree.

The final candidate only skips launching a directory-enumeration shell when
`profile.d` contains no batch scripts. The existing directory traversal and
script order are retained whenever scripts exist. Git auto-selection, PATH
placement, alias migration, user startup scripts, Clink and the prompt remain
on their original paths. It does not enable `/f` or pin a different Git.

Broader candidates replaced shell-detection pipelines, read alias/version files
directly, and combined PATH regex queries. They did not demonstrate a stable
improvement: an alternating-order comparison of 10 starts per variant had median
1400.76 ms / P95 1426.46 ms for the baseline and 1474.17 ms / 1882.21 ms for the
broader candidate. Those changes were discarded. Removing subprocesses alone is
not evidence of a win. The final minimal candidate is measured separately below.

Final result, 10 alternating-order pairs on an idle machine (nearest-rank P95):

| Cmder variant | Starts | Prompt P50 | Prompt P95 | Pane preparation P50 |
| --- | ---: | ---: | ---: | ---: |
| Installed-source copy | 10 | 1409.81 ms | 1480.61 ms | 26.21 ms |
| Empty-profile-directory guard | 10 | 1366.49 ms | 1425.20 ms | 26.18 ms |

The measured median improvement is about 43 ms (3.1%). This is a modest local
result, not proof of stability across machines or a 500 ms result. Both rows use
the Hitty changes in this PR: the difference measures the isolated Cmder patch,
not the performance effect of Hitty's configuration/history changes. All 20 starts
passed Tab completion, alias execution and native history replay. Raw timings,
summary and source hashes are in [the evidence record](terminal-startup-evidence.json).

The measurement uses the real `DaemonPane::spawn`, shell integration and bundled
ConPTY. It starts timing before pane preparation and stops at the first active
prompt report; every sample must then complete a DOSKEY alias using Tab, execute
it, and execute it again through Up+Enter. This is a daemon/shell measurement,
not the GUI click or first painted frame. History files in each fixture begin
empty and accumulate within that fixture.

The executable must have the same `conpty.dll` and `OpenConsole.exe` as Hitty
beside it. An initial attempt using Windows' in-box ConPTY stalled during input
validation and was excluded, not treated as a passing sample. The harness also
clears inherited Cmder/Clink initialization flags and alias paths before starting
workers: an Agent launched inside Cmder otherwise skips initialization and can
reference the real user profile.

## Reproduce on Windows

Use an idle machine for measurements, without builds or tests running in parallel.
Run from the repository root. Use a new fixture output directory for each setup.

```powershell
cargo build --locked -p hitty-core --example measure-pane-startup --no-default-features
# Use the assets produced by the normal Hitty build, not a different ConPTY.
Copy-Item target/debug/conpty.dll target/debug/examples/conpty.dll
Copy-Item target/debug/OpenConsole.exe target/debug/examples/OpenConsole.exe
python scripts/prepare-cmder-startup.py D:/tools/cmder_full/cmder .scratch/cmder-startup --samples 20 --budget-ms 500
python scripts/verify-cmder-startup.py .scratch/cmder-startup
target/debug/examples/measure-pane-startup.exe .scratch/cmder-startup/baseline/options.json
target/debug/examples/measure-pane-startup.exe .scratch/cmder-startup/optimized/options.json
```

The optional budget returns a failure when nearest-rank P95 exceeds the budget;
measurements are saved before that failure. Omitting it produces measurements
without a performance gate. A command/input timeout also fails and records the
failed sample rather than silently dropping it. Source file hashes are saved in
`source-hashes.json` so the locally installed baseline can be identified.

## Validation

- Locked workspace/updater tests: 3341 passed, 14 ignored, no failures. Optional
  installed-Cmder tests may skip internally; the direct fixture measurements
  below are the explicit installed-runtime coverage.
- Locked workspace/all-target check with updater: passed.
- Rust formatting, Agent manifest synchronization and its four Python tests:
  passed.
- Native application and CLI smoke with isolated user data: command input/output,
  wait-free CLI and GUI restart/session reconnect passed.
- 19 baseline/candidate behavior comparisons: PATH placement and case handling,
  shell detection, legacy alias migration, and empty/populated profile scripts.
- No visual layout change; no image-based visual acceptance is claimed.

The installed Clink executable reports 1.2.46.69fc92; bundled ConPTY product
version is 1.24.260710001. Dependency versions and pins were not changed.

## Remaining acceptance

Measure the project-group sidebar new-terminal button and split-pane entry point
using the same configuration, directory and terminal settings. Separate daemon
cold start from additional panes in an already-running app; report P50, P95,
maximum and failures. Include non-Git and Git directories, first command,
completion, history, prompt refresh and Ctrl+C. The company machine's custom
synchronous `git branch --show-current` prompt is not present in the local
configuration, so its proposed asynchronous replacement is not verified here.
