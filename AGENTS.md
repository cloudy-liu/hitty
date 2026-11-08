# Hitty contributor guidance

## Repository and scope

Use `cloudy-liu/hitty` for issues, pull requests, and releases. The `origin`
remote is this repository. Default to squash merging pull requests.

For bootstrap and branding work, read `docs/hitty-bootstrap-plan.md` and
`docs/adr/0001-independent-source-snapshot.md`. Use `GLOSSARY.md` for product
terminology. Preserve source provenance and third-party licenses.

Keep dependency versions, pinned Git revisions, and CI action versions aligned
with the reviewed source baseline. Upgrade them only for a separate maintainer
request with validation; Dependabot version-update PRs are disabled.

## Validation

Use locked Cargo builds and the existing workspace and updater tests. Exercise
the affected public application or CLI behavior with isolated user data.

For visual changes, capture actual Before and After application images from the
same reproducible state, viewport, display scale, and settings. Reuse existing
feature scripts to drive `scripts/capture-native-window.ps1` (PrintWindow) for
Windows single-window captures. It runs unattended without moving the real
pointer. Use LCU only for scenes outside that window (tray, taskbar, system
dialogs) or when PrintWindow cannot render them. Inspect saved images; for LCU,
move the pointer outside the capture first and repeat if it remains visible.
Keep durable evidence under `docs/testing/images/` and embed the actual images
in PR evidence. If required captures are unavailable, mark visual acceptance
unverified and keep the PR draft until resolved or explicitly accepted.

For this bootstrap, treat the selected source snapshot as the accepted functional
baseline. Validate the rename and affected public flows using the inherited
harnesses; keep broader feature work separate. Preserve the separate source
baseline commit when integrating the bootstrap; later PRs default to squash.

Before creating or updating a PR, use the `mattpocock-skills:pr` skill and its
Summary, Evidence, and Merge Danger structure.

## Releases

Assemble releases as drafts. Verify all expected platform assets and checksums,
then publish explicitly after the agreed platform acceptance is complete.
Release notes are English and describe user-visible changes and limitations.
Compare subsequent releases against Hitty tags; the first records its source
baseline. Keep build logs and CI run IDs in verification records, not release
notes.
