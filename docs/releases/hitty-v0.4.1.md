# Hitty v0.4.1

Reduce repeated work when opening terminals and prepare command history away
from the UI thread.

## Highlights

- New panes use one fresh configuration snapshot and prepare their complete
  command history alongside shell creation (cloudy-liu/hitty#47).

## Changes

### Changed

- Reuse one configuration snapshot throughout a pane's startup. Each new pane
  still reloads settings, so subsequent panes observe configuration changes
  (cloudy-liu/hitty#47).
- Load and rank command history on a worker while the shell is created, then
  hand the complete history to the terminal for suggestions and search
  (cloudy-liu/hitty#47).

### Internal

- Add reproducible startup measurements and behavior-checked Cmder comparison
  tools that operate on explicitly generated isolated copies
  (cloudy-liu/hitty#47).

## Known limitations

- Startup time depends on the configured shell. The development measurements
  isolate configuration preparation and a separate Cmder candidate; they do
  not establish an end-to-end release-build speedup or a sub-500 ms prompt.
  Installed Cmder configuration requires separate acceptance before applying
  the candidate changes (cloudy-liu/hitty#47).
- Windows packages are unsigned. macOS packages use ad hoc signatures and are
  not notarized; operating-system download protection may require an explicit
  first-launch exception.
- Linux retains its HTML source view rather than an embedded HTML preview.
- Herdr-core remains optional and separately installed; its runtime acceptance
  covers Windows, with macOS/Linux integration limitations retained.

**Full changelog:** [v0.4.0...v0.4.1](https://github.com/cloudy-liu/hitty/compare/v0.4.0...v0.4.1)
