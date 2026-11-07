# Full-file diff context

Review a change while reading the surrounding file, with additions and
deletions still highlighted.

- Use icon buttons in the file title and focused toolbar, with localized
  tooltips, rather than persistent text controls. Use distinct unfold/fold
  glyphs for context and keep the existing layout icon.
- A file-level unfold action expands that file in place and reveals its full
  context without changing overlay focus or hiding the other file cards. Fold
  returns to changed regions. Keep the source line at the same screen position
  when possible; if folding removes it, retain the nearest available line
  instead of jumping to the file's start.
- Unified/split layout is independent of context. Provide previous/next
  change arrows in the focused toolbar and 20-line expansion at gap edges.
- Fetch through HostOps and the existing DiffSource Git arguments, including
  literal old/new paths for renames. Preserve staged, unstaged, HEAD, commit
  and merge-base range semantics. Do not substitute disk bytes.
- Fetch one file on demand. Keep the overview snapshot and its totals intact.
  Retain at most 50,000 lines and explain truncation. Hide context actions for
  binary files. Keep the existing patch visible on failure.
- Ignore stale replies after switching files, refreshing or closing/reopening
  an overlay. Capture real Computer Use before/after screenshots using the
  same repository and file.

Automated checks cover source-version fidelity, literal and renamed paths,
context projection and native GPUI position retention in both layouts. Manual
checks cover unfold, fold, partial context, layout and change navigation.
