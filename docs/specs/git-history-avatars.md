# Git History: compact author rows and shared commit diffs

## Problem Statement

Git History compresses each commit into a single line. Commit subjects compete
with refs and age, authors are hidden in tooltips, and authors have no avatar.
The result is harder to scan and less visually clear than the reference the
user supplied. Adding remote images must preserve responsive scrolling and
offline use.

## Solution

Adopt the approved compact prototype: a circular 28px author avatar beside a
two-line, 44px commit row. The full subject, including its conventional-commit
scope, occupies the first line. Author, relative age, and refs occupy the second.
Keep graph lanes, commit nodes, merge/root shapes, branch labels, release tags,
selection, hover, filtering, paging, and resizing.

Selecting a commit opens its changes in the existing Git Diff View and leaves
History available for selecting another commit. Reuse its existing view-mode
icon, unified/side-by-side preference, navigation, and document controls.
The view is an independent `Commit changes` document tab alongside open files,
with its own breadcrumb and message-detail header. The header shows the full
subject, author photo/name, authored date, copyable SHA, refs, message body and
loaded change totals. Switching back to a file retains both documents; closing
the commit page or using the document-close shortcut preserves the terminal.

## User Stories

1. As a developer, I want commit subjects on their own line, so that I can
   distinguish changes quickly.
2. As a developer, I want the subject's scope preserved, so that I know which
   part of the project changed.
3. As a developer, I want an author avatar, so that I can recognize contributors.
4. As a developer, I want author and age beneath the subject, so that context
   remains visible without opening a tooltip.
5. As a developer, I want long subjects truncated with a complete tooltip, so
   that a narrow sidebar remains usable.
6. As a developer, I want stable circular initials while an image loads, so that
   a slow connection does not leave an empty or shifting row.
7. As an offline developer, I want cached avatars or initials, so that History
   remains useful without a connection.
8. As a developer, I want repeated authors to share images and requests, so
   that a long history does not repeatedly download the same avatar.
9. As a developer, I want bounded image size, concurrency, and cache capacity,
   so that browsing history does not cause unbounded resource use.
10. As a developer, I want failed avatar requests retried conservatively, so
    that scrolling does not repeatedly hit an unavailable service.
11. As a developer, I want branch lanes and commit nodes retained, so that
    graph relationships remain visible.
12. As a developer, I want merge and root nodes retained, so that topology
    remains understandable after the row height changes.
13. As a developer, I want branch and Tag labels retained, so that refs remain
    discoverable alongside the new author information.
14. As a developer, I want excess refs represented with an overflow count and
    complete names available on hover, so that labels do not consume the subject.
15. As a developer, I want selecting a commit to show that commit's changes in
    the existing Git Diff View, so that the controls behave consistently.
16. As a developer, I want History to remain available while viewing a commit,
    so that I can move between commits without reopening the section.
17. As a developer, I want the existing diff mode and icon reused, so that the
    new entry point does not create another diff viewer or preference.
18. As a developer, I want filtering and load-more to continue working, so that
    visual changes do not remove navigation capabilities.
19. As a developer, I want a 260px sidebar to remain usable, so that this change
    does not require a wider workspace.
20. As a developer, I want light/dark surfaces, selection, and hover to follow
    the application theme, so that History fits the rest of the interface.
21. As a developer, I want remote avatars to be optional, so that I can use
    local initials without making external image requests.
22. As a developer, I want Git data and image work kept off the UI thread, so
    that terminal interaction and scrolling remain responsive.

## Implementation Decisions

- Use the approved compact geometry as the production default. The prototype's
  current-style/reference-style comparison controls are review tools only.
- Retain fixed-height virtualization and the existing single graph canvas.
  Adjust lane geometry and visible-row calculations together with row pitch.
- Reuse the existing avatar component for photos and local initials.
- Resolve known GitHub no-reply identities to public avatars. Use Gravatar's
  hashed-email image endpoint for other valid addresses, with local initials
  when no image is available. Do not infer accounts from display names.
- Use the application's existing HTTP client and proxy behavior. Require no
  account, token, OAuth flow, or provider-specific login.
- Begin image work only for materialized rows; share work by normalized author
  identity. Limit concurrent work and retain a bounded number of decoded images.
- Request small static images, limit response bytes and decoded dimensions,
  and normalize successful images to a fixed small bitmap before caching.
- Use a bounded memory cache and ordinary files in the application's own cache
  directory. Persist successful images across restarts, refresh expired entries,
  retain stale images when offline, and briefly cache failures in memory.
- Keep avatar loading independent of Git history freshness and repository RPCs.
  Image completion must not rerun Git or change a row's geometry.
- Provide a persistent remote-avatar switch in History's existing menu.
- Preserve ref priorities and overflow counts; expose complete ref names on hover.
- Route a selected commit to the existing Git Diff View using its real repository,
  revision, and commit label. Preserve existing context-menu Git actions.
- Keep the current History section's default footprint; taller rows intentionally
  show fewer commits, and the existing height divider remains available.
- Follow-up clarification: preserve the prototype's separate document page,
  rather than presenting only the existing compact Diff toolbar. Reuse file-tab
  chrome and the diff renderer; selecting another commit updates the commit
  document. Bound long titles/messages and allow the message body to expand.

## Testing Decisions

- Confirmed seam 1: existing SCM rendering and click interactions. Exercise
  visible subject/author/ref presentation, fixed geometry at narrow widths,
  virtualization, selection, and opening the correct existing diff source.
- Confirmed seam 2: the avatar-loading entry point. Control HTTP at the system
  boundary and use real temporary cache files to verify request sharing,
  image limits, failure fallback, bounded work, and offline cache reuse.
- The user confirmed both seams before feature tests were written.
- Extend existing GPUI interaction tests and HTTP fake-client patterns; assert
  observable behavior rather than helper implementation or private call chains.
- Use a controlled real Git repository and the running production application
  for matching Before/After captures at the same viewport, scale, theme, and
  History height. Capture through LCU and preserve screenshots in the PR.
- Run focused regressions during implementation, regular typechecking, and the
  full workspace suite once after integration. Review standards and spec in
  separate review agents before committing the final result.
- Publish matching Before/After performance measurements in the PR: native
  History scroll CPU-frame median/P95, avatar cold/warm request counts and peak
  concurrency. State commit count, viewport, display scale, build settings,
  sampling method, and that CPU-frame timing excludes GPU execution/presentation.
  Measure a long loaded history as well as the visual fixture; do not present
  HTML prototype timing or estimates as native application measurements.

## Out of Scope

- A second diff viewer, alternate diff mode preference, or redesigned diff engine.
- GitHub/GitLab login, account search, multi-provider identity synchronization,
  or guaranteed matching of every arbitrary commit email to a GitHub account.
- Animated/high-resolution avatars, cache databases, or preloading all history.
- Changing the existing Git context actions, graph scope semantics, or page cap.
- Shipping the prototype's compare toolbar, synthetic sessions, illustrative
  graph data, or sample-only uncommitted-changes row.
- Unrelated Diff context-control changes already present in the user's worktree.

## Further Notes

The user approved the compact HTML prototype and requested one implementation
ticket, implementation, review, commit, and a PR. The project's tracker and PR
destination are cloudy-liu/ctty7. Preserve the parent spec when completing the
implementation ticket. If native visual capture is blocked, retain a draft PR
and record the exact blocker and unverified visual acceptance.
