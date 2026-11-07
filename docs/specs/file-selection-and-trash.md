# Spec: Multiple file selection and recoverable deletion

## Problem Statement

The Files panel selects only one file or folder at a time. A user who wants to
delete a group must repeat the operation for each item. The panel currently
permanently removes selected files, including the contents of selected folders,
instead of placing them in a recoverable trash location.

The ordinary file tree and the Native SSH SFTP browser are separate surfaces
behind the Files panel. Their interaction should be consistent, while file
operations must act on the machine that owns the selected items.

## Solution

Provide familiar range and additive selection for files and folders in the
Files panel. A user-initiated delete shows a confirmation dialog with a
"Don't ask again" checkbox for single items or batches. Confirming with the
checkbox selected persists the preference and skips future confirmations.
Cancelling does not delete or save the checkbox choice. Items move to the
owning system's recoverable trash. If a target cannot be moved to appropriate trash,
preserve it and explain the failure; never fall back to permanent deletion.

## User Stories

1. As a user, I want to select a file and Shift-click another, so that I can select the visible range between them.
2. As a user, I want to select folders and files together, so that I can remove a mixed group in one operation.
3. As a user, I want Ctrl-click on Windows and Linux or Command-click on macOS to toggle individual items, so that I can select nonadjacent entries.
4. As a keyboard user, I want Shift plus Up or Down to extend the selection, so that I can select a range without a mouse.
5. As a keyboard user, I want Delete to act on the selection while the list has focus, so that I can delete it without opening a menu.
6. As a user, I want an unmodified click in the ordinary tree to retain its existing file-opening and folder-expansion behavior, so that normal browsing continues to work.
7. As a user, I want modified selection clicks to avoid opening files or changing folder expansion, so that selecting does not disturb the list.
8. As a user, I want every selected item to be visibly highlighted, so that I can check the selection before deleting.
9. As a user, I want repeated Shift selections to use a stable anchor, so that I can expand or shrink a range predictably.
10. As a user, I want the range to follow the displayed ordering and exclude non-file placeholder rows, so that the highlighted result matches what I see.
11. As a user, I want selection to work in file-search results as well as the normal listing, so that I can act on matching items together.
12. As a user, I want items hidden by a fold, filter, navigation, or listing change removed from the actionable selection, so that I do not delete invisible items accidentally.
13. As a user, I want right-clicking a selected item to retain the group, so that a context-menu delete acts on that group.
14. As a user, I want right-clicking an unselected item to target that item alone, so that an old selection cannot cause an unexpected batch operation.
15. As a user, I want actions that make sense only for one item to be unavailable for a multi-selection, so that their target is unambiguous.
16. As a user, I want the existing project-root protection preserved, so that selecting a range cannot bypass it.
17. As a user, I want deletion to ask for confirmation by default and offer a Don't ask again checkbox, so that I can choose the interruption level I prefer.
18. As a user, I want deleting a selected folder to move the folder and its contents to trash, so that I can recover the complete folder later.
19. As a user, I want the deletion targets fixed when I invoke delete, so that changing tabs, hosts, or selection cannot redirect the operation.
20. As a user, I want each underlying target processed only once, even if a tree shows it more than once or a selected folder contains another selected item.
21. As a user, I want deleting a symbolic link to move the link itself, so that its target is not unexpectedly removed.
22. As a user, I want a deleted local file or folder moved to the system trash with its original location retained, so that I can restore it using the system's normal tools.
23. As a remote workspace user, I want a remote target moved to recoverable trash on its own host, so that a matching local path is never affected.
24. As a Native SSH user, I want the SFTP list to follow the same selection and recoverable-deletion rules, so that the Files panel behaves consistently.
25. As a user on a host or volume without usable trash, I want the item preserved and the limitation explained, so that delete never becomes permanent without my knowledge.
26. As a user, I want failures reported per item and successful items reflected accurately, so that I know what remains and can retry it.
27. As a user, I want ordinary successful deletions to avoid blocking dialogs, so that I can continue working without interruption.
28. As an editor user, I want unsaved buffers and existing external-change safeguards preserved, so that moving an open file to trash does not silently discard edits.
29. As a user switching sessions or browsing another host, I want selection and operations scoped to their owning context, so that a batch does not leak into another workspace.
30. As a user of any supported interface language, I want the new selection and failure text localized, so that the action and its consequences are clear.
31. As a Native SSH user, I want one click to select a folder and a double click to enter it, so that I can use a folder as the start of a Windows-style selection range.
32. As a user, I want Don't ask again saved only when I confirm, so that cancelling does not change my future deletion behavior.
33. As a user, I want this preference remembered across application restarts and shared by both Files panel surfaces, so that my choice is consistent.

## Implementation Decisions

- Cover explicit user-facing file/folder deletion from the right-side ordinary file tree and Native SSH SFTP list, including context-menu and keyboard entry points. This is not a global change to every internal filesystem removal.
- Track selected row identities, an anchor, and a keyboard focus/current row per owning session and host. Use the current rendered ordering for a Shift range, including search results, and retain the anchor through repeated range changes. Ignore loading/empty/error notes. Keep selected paths that merely scroll out of the viewport; prune entries that leave the actual listing through folding, filtering, navigation, or refresh. Reset an invalid anchor safely.
- In the ordinary tree, use ordinary click for the existing activate/open/fold behavior. In the Native SSH list, a plain click selects a folder and a double click enters it; file activation remains unchanged. This resolves the conflict between immediate folder navigation and choosing a folder as the first item of a range. Modifier-click only changes selection. Ctrl/Command toggles an individual row and Shift-click or Shift-arrow extends the range. Keep focus and input editing isolated so Delete cannot fire while the user types in a search or rename field.
- Right-click on a selected row operates on the selected set; right-click on an unselected row selects and targets only it. Present only meaningful group actions for multiple selections. Preserve root protection and do not allow a range, duplicate root occurrence, keyboard shortcut, or context menu to bypass it.
- Snapshot the exact eligible targets and owning host/session when delete is invoked. Deduplicate repeated occurrences and suppress descendants of a selected directory when that directory is itself a target. Do not follow a symlink in order to delete its target. Handle stale, missing, or changed targets safely and report them.
- Use one shared confirmation flow for single and multiple selections in both Files panel surfaces. Default to asking, including when older configurations omit the preference. Show target identities/count and the owning host, plus a Don't ask again checkbox initially unchecked. Confirming with it checked persistently disables later file-delete confirmations; cancelling, Escape, or dismissing the dialog performs no operation and does not save the choice. Snapshot targets before opening the dialog. After confirmation, or when the preference skips it, request recoverable trash without a second platform confirmation or blocking success dialog. Selected directories enter trash with their contents. If an OS/provider would require permanent deletion, abort and report the failure; never automatically accept such a prompt or offer a permanent-delete fallback.
- Add a recoverable-trash capability separate from the existing permanent removal primitive, because the latter is also used for temporary staging and internal cleanup. Route single and batch user delete actions through this capability; do not redefine existing internal removal semantics globally.
- Use the owning platform's real trash mechanism where supported. Windows and macOS should use their native recycle/trash facilities; Linux should use an implementation compatible with the [freedesktop.org Trash Specification](https://specifications.freedesktop.org/trash/latest/). A successful result must be recoverable using the owning system's normal tools, including original-location metadata. Deal safely with collisions, permissions, links, full/unsupported volumes, and provider cancellation; a request to recycle must never authorize permanent destruction if the platform cannot recycle an item.
- For remote hosts and WSL, execute the trash operation on the owning host, never on the client by interpreting a remote path locally. Support a known-good Linux remote workspace and a Native SSH/SFTP Linux home directory with a usable freedesktop-compatible trash, including recoverable files and nonempty folders. Detect unsupported hosts, old peers, or inaccessible trash before deleting and preserve the affected original. The SFTP protocol alone does not guarantee a trash facility: do not claim every server supports one or silently substitute unlink/recursive remove. Preserve compatible behavior with older peers by failing safely rather than downgrading deletion. Do not interpolate file names into remote shell commands.
- Keep the interface responsive while processing a batch. Track outcomes per top-level target, refresh all affected listings, clear successful selections, and retain or reselect failed visible targets for retry. Explain successes, failures, and unknown outcomes without claiming atomicity. If a connection drops after a request was sent, reconcile the state before offering a retry; do not report a definite success or failure without evidence.
- Preserve editor dirty-buffer and external-change handling, including files beneath a selected folder. Localize new copy in the existing supported languages and update file-panel/remote documentation to describe recoverability and limitations.

## Testing Decisions

- User-confirmed seam: reuse the existing GPUI application-window harness and exercise real list clicks, keyboard input, context-menu targeting, confirmation and observable results with temporary fixtures. Favor behavior visible to users over tests of private selection helpers. Reuse existing file-tree keyboard, nested-root, editing, watcher, and SFTP test patterns.
- Cover forward/backward ranges, repeated Shift selection, Ctrl/Command toggles, Shift-arrow, single-click behavior, active search, folds, hidden rows, root protection, duplicate rows, mixed folders/files, focus and input fields, and tab/host changes. Check right-click both inside and outside the selection.
- Verify default single/batch confirmation, confirming with or without Don't ask again, cancellation/dismissal without preference changes, persisted skip behavior after restart, and sharing across tree/SFTP. Ensure snapshots cannot retarget while the dialog is open. Exercise duplicate and ancestor/descendant targets, special characters, symlinks, missing items, mixed success/failure, and a disconnected or unsupported host. Verify unchanged originals whenever trash is unavailable and verify that permanent removal is never called as a fallback.
- At the platform/host boundary, use isolated disposable files and an isolated trash test environment where supported to move and then actually restore a file and nonempty directory, verifying their original locations and contents. Cover metadata and name collisions as well. Exercise a known-good Linux remote workspace and Native SSH/SFTP fixture as well as unsupported servers. Independently verify Windows, macOS, Linux, remote workspace/WSL, and Native SSH capabilities where environments are available; record unverified environments instead of claiming parity.
- Verify open dirty documents retain their edits and existing external-change safeguards after a target or its parent folder is moved to trash. Exercise older remote peers and interrupted requests without acting on unrelated user files.
- For the implementation PR, follow the repository's native Before/After screenshot gate with the same fixture, viewport, scale, and relevant settings. Show the selection and resulting listing after deletion; use actual running-application captures rather than mockups.

## Out of Scope

- A built-in trash browser, restore command, trash emptying, automatic purge policy, or permanent-delete option.
- Commands run directly in a terminal or filesystem operations initiated outside the Files panel.
- Replacing a destination through copy/drop and deleting application-owned temporary or staging files; those retain their existing, separately scoped semantics.
- General bulk rename, bulk copy/move, or multi-file drag-and-drop.

## Further Notes

The user requested range selection like a normal file manager and specifically
requires recoverable trash, with a default confirmation dialog and a persistent
Don't ask again checkbox as the latest instruction. This supersedes the earlier
no-confirmation requirement. The user confirmed the existing GPUI window-test seam and
separate platform/remote checks. Restore uses the owning system's normal tools;
remote items are restored on the remote host. At ticket creation, the Native SSH
folder gesture above was chosen as the working assumption to match the user's
request for Windows-like selection; the user may still adjust it. This
specification describes the desired behavior; it does not claim that the
capability is already implemented or that every remote server has recoverable
trash.

Reference behavior: [Windows IFileOperation recycle flag](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-setoperationflags), [macOS FileManager trash operation](https://developer.apple.com/documentation/foundation/filemanager/trashitem%28at%3Aresultingitemurl%3A%29?language=objc), and the [SSH file-transfer protocol draft](https://datatracker.ietf.org/doc/html/draft-spaghetti-sshm-filexfer).
