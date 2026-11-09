# Hitty v0.2.0

A cleaner right panel and a contextual Outline make it easier to navigate
documents, Agent conversations and terminal commands within your existing sessions.

## Highlights

- Switch between **Info, Outline, Git and Files** with compact icons and a label
  on the active mode.
- Navigate Markdown headings, including formatted, repeated and non-Latin
  headings. Local HTML previews on Windows and macOS expose their visible
  headings too.
- Revisit Agent turns or recorded shell commands from Outline. Selecting a
  command scrolls to its output; it does not run the command again.
- Toggle hidden files and refresh the file tree directly beside the collapsible
  project root. Hitty's colorful file icons, search and context menus remain.
- Keep document tabs, edits and navigation within the owning session. Right-panel
  actions do not create or switch sessions in the left sidebar.

## Other improvements

- Refreshed English and Chinese project introductions, installation guidance
  and product screenshots.
- Removed obsolete documentation and repaired references while retaining
  licenses and source provenance.

## Downloads and limitations

- **Windows x86_64:** Setup installer or portable ZIP.
- **Linux x86_64:** AppImage or tar.gz archive.
- **macOS:** DMG and ZIP for Apple Silicon (arm64) and Intel (x86_64).
- Remote-server binaries are included for Linux and macOS on both architectures.

Windows packages are unsigned. macOS packages use ad-hoc signatures and are not
notarized; operating-system download protection may require an explicit first-launch
exception. Linux keeps its existing HTML source view rather than an embedded
HTML preview.

Shell command outlines require shell-integration markers with command text.
Expired scrollback and full-screen terminal content may have no available jump
target. Existing settings and session behavior are retained.

[All changes since v0.1.0](https://github.com/cloudy-liu/hitty/compare/v0.1.0...v0.2.0).
