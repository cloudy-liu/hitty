# Full-file diff context verification

The diff viewer can unfold a file while retaining additions, deletions and both
line-number columns. Context and unified/split layout are independent.

## Native Windows evidence

The screenshots use an isolated application configuration and the same Git
fixture before and after the change. `messages.rs` has 80 lines, with replacements
at lines 28 and 58. Both versions were launched as native Windows applications.
Controls were clicked through Computer Use; these are application captures.

| Capture | State |
| --- | --- |
| [Before](diff-context-images/before.png) | Only compact hunks, starting at lines 25 and 55 |
| [Full context](diff-context-images/full-unified.png) | Continuous surrounding code with additions and deletions retained |
| [Split layout](diff-context-images/full-split.png) | Full context remains active after changing layout |
| [Partial context](diff-context-images/context-20-lines.png) | Revealing 20 preceding lines moves the first hunk start from 25 to 5 |

Manual checks cover the file-title unfold button, toolbar fold/unfold, unified
and split layout, previous/next change navigation, docked/full workspace views
and gap expansion. File-level expansion keeps the other cards in the list and
does not change the current file focus. The first replacement retains its
screen position while expanding and folding in the docked viewer.

## Automated coverage

On Windows, put Git's `usr/bin` directory on the test process's PATH. Existing
remote-link tests spawn `cat` and `sleep`; PowerShell aliases cannot satisfy
those subprocesses.

```powershell
$env:Path = 'C:\Program Files\Git\usr\bin;' + $env:Path
cargo test --workspace --locked -- --test-threads=1
```

The application suite passed with 1,730 tests and 7 ignored fixtures/benchmarks;
the core suite passed with 1,200 tests and 5 ignored fixtures/benchmarks, and the
CLI suite passed with 138 tests.
The native Windows build, `cargo fmt --all -- --check`, `git diff --check` and
`.github/scripts/check-host-boundary.sh` also passed. The HostOps check scanned
103 UI/terminal source files without finding direct filesystem or Git access.

- `full_file_preserves_index_worktree_and_historical_versions` checks Worktree,
  Staged, HEAD, commit and range comparisons against their original versions,
  including literal paths and renamed files.
- `full_file_reads_pure_rename_and_mode_changes_from_the_compared_blob` checks
  metadata-only changes, symlinks, index/worktree disagreement and ambiguous
  `0:` paths on platforms that support them.
- Context projection preserves distant additions and deletions and reveals only
  requested context ranges.
- `full_file_controls_preserve_change_position_and_layout` checks expansion and
  folding positions in both layouts, partial context and stale-request
  invalidation on file switches.

Full-file loading retains at most 50,000 lines and uses the existing truncation
notice. The overview snapshot and totals are preserved. No direct filesystem or
Git access was introduced in the UI layer.

## Review

### Standards

No remaining documented-standard violations or maintenance findings. The final
coordinate/deferred-scroll delta was reviewed separately after the initial pass.

### Spec

No remaining findings against `docs/specs/diff-full-file.md`. Review fixes cover
stale reads, metadata-only content and explicit index-stage path syntax.

The over-engineering review has no remaining cuts: "Lean already. Ship."

## Limits of this verification

Native interaction was checked on Windows. macOS, Linux and a live SSH host were
not manually exercised. Remote loading follows the existing HostOps boundary.
This work builds on the viewport rendering change in cloudy-liu/ctty7#154.
