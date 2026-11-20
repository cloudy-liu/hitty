# Hitty v0.5.0

Review Git diffs with fixed controls and bulk file folding, keep Codex colors
consistent across themes, and restore local Qoder conversations without hooks.

## Highlights

- Collapse or expand Diff file cards with one icon-only button, while keeping
  the file you are reading visible (cloudy-liu/hitty#55).
- Keep existing Codex sessions visually aligned with fresh sessions after
  switching One Light and One Dark Pro (cloudy-liu/hitty#50).
- Resume each local Qoder or Qoder CN conversation in its original pane after
  a restart or update, without installing hooks (cloudy-liu/hitty#51).

## Changes

### Added

- Add bulk file folding beside the layout control in worktree and commit
  diffs, using the existing expand/collapse icons and localized tooltips
  (cloudy-liu/hitty#55).

### Changed

- Keep commit Diff controls and file totals in a fixed toolbar above the
  scrolling file list, with reading-position preservation across folding and
  layout changes (cloudy-liu/hitty#55).

### Fixed

- Adapt known Codex background fills to the current theme while preserving
  the native distinction between submitted prompts, plans, and editable input
  (cloudy-liu/hitty#50).
- Recover exact local Qoder and Qoder CN conversation IDs from their live
  process logs and saved history, preserving the configuration directory and
  captured launch options without replaying the initial prompt. A new empty
  conversation clears its previous restore target (cloudy-liu/hitty#51).

## Known limitations

- Diff rendering limits and automatic folding protections remain in place;
  bulk expansion applies to eligible rendered file cards. Native visual
  acceptance for the new controls covers Windows (cloudy-liu/hitty#55).
- Exact Codex fill correction requires a detected Codex pane, readable
  terminal colors, and an observed terminal-background reply. Native visual
  acceptance used Codex CLI 0.169.0 on Windows (cloudy-liu/hitty#50).
- Qoder recovery requires a readable session log and saved conversation.
  Restart/update acceptance used isolated process fixtures; authenticated
  live conversation restoration was not exercised. Custom launches that
  bypass command capture may omit original flags (cloudy-liu/hitty#51).
- Windows packages are unsigned. macOS packages use ad hoc signatures and are
  not notarized; operating-system download protection may require an explicit
  first-launch exception.
- Linux retains its HTML source view rather than an embedded HTML preview.
- Herdr-core remains optional and separately installed; its runtime acceptance
  covers Windows, with macOS/Linux integration limitations retained.

**Full changelog:** [v0.4.1...v0.5.0](https://github.com/cloudy-liu/hitty/compare/v0.4.1...v0.5.0)
