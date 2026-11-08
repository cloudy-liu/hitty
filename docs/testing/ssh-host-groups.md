# SSH host groups in the sidebar

In-pane SSH hops no longer sit in Scratch. Each connection's display identity
becomes its own sidebar group between the local project groups and Scratch,
with an `SSH` chip on the header; local shells without a repository keep
Scratch. Pinning and "New tab here" are not offered for these groups.

## Current Windows acceptance

These are unedited Local Computer Use captures of the running production GPUI
application on 2026-10-31. Both use the same deterministic six-session fixture,
light theme, 1200 x 660 logical viewport, 100% display scale, sidebar grouping
set to Repo, and active `Fix login` session. No SSH connection is opened.

| Phase | Source | Capture | Binary SHA-256 |
| --- | --- | --- | --- |
| Before | base `5ec55456`, with this branch's fixture overlaid | [before](images/ssh-host-groups/before-windows-current.jpg) | `778cc67ff07623fa415e021cdcd417b9e68fd55d68bb39bb810f20583653407d` |
| After | feature head `25772ed0` | [after](images/ssh-host-groups/after-windows-current.jpg) | `9c3d3a5ea8f4316a9249c732a6bff4178942b525fa6ae7fe1cf37da05e51d507` |

To make Scratch visible on both versions, both builds additionally set the
first local tab's existing `sidebar_group` fixture cache to
`Some(PathBuf::from("C:/visual/Project"))` immediately after `Tab::new`. The
application's grouping and rendering code is unchanged. Reproduce the base
fixture overlay with `scripts/ssh-tabs-native-visual.py prepare-before`, apply
the same one-line fixture assignment in both worktrees, and build with
`cargo build --locked --features markdown-visual-tests --bin tty7-app`. Launch
with `--ssh-tab-visual` and the options documented below.

Before, the three SSH sessions appear in Scratch. After, `prod-web` and
`dev-box` have their own groups with outline SSH pills, while the two remaining
local sessions stay in Scratch. The cached Project group is fixture data rather
than a live repository probe. The before/after image SHA-256 values are
`c35fbdfadebcfedfd5485a85a65c5ae6ed0116aff1a7e655aa0415b8e30a4cac` and
`3b0aef9c6c52ad18935df2532436507017dabcc93575bf791b5087a60625567b`.

## Current-head Linux CI preview (supplementary)

The following unedited screenshots come from the production GPUI application in
[CI run 41070303035](https://github.com/cloudy-liu/ctty7/actions/runs/41070303035).
Both use the same deterministic SSH fixture, light theme, Linux/X11 display
settings and 1200 x 660 window; no network SSH connection is opened. The before image uses
base `5ec55456`, and the after image uses CI merge commit `a2973bb7` with
parents `5ec55456` and feature head `cf87a514`.

- [Before: SSH sessions in the flat sidebar](images/ssh-host-groups/before-ci-sidebar.png)
- [After: per-host groups with the transparent SSH pill](images/ssh-host-groups/after-ci-sidebar.png)

These images are supplementary previews. The older Windows captures below
predate the rebase and the transparent pill.

## Historical native UI evidence

Captured on Windows with Local Computer Use on 2026-10-30. Both phases run the
production GPUI application through the opt-in `--ssh-tab-visual` fixture with
identical deterministic daemon messages (local terminal / Codex / Claude Code,
native SSH `prod-web`, terminal SSH `Fix login` and native SSH `Deploy check`
on `dev-box`), `sidebar: true`, light theme, a 1200 × 660 logical window at
100 % display scale (captures are 1202 × 661 pixels including the frame). No
network SSH connection is opened. Application pixels were not redrawn.

| Phase | Source | Capture |
| --- | --- | --- |
| Before | `f8b9afcd` (`main`) with this branch's fixture overlay | [before-sidebar.jpg](images/ssh-host-groups/before-sidebar.jpg) |
| After | `efd79855`, the pre-rebase form of this branch's feature commit (rebased onto #227 and #229 as `947de8fe` with no behaviour change) | [after-sidebar.jpg](images/ssh-host-groups/after-sidebar.jpg) |

**Before:** every session sits in one headerless list; the three SSH sessions
are mixed with the local ones and nothing marks a host.

**After:** `prod-web` (1) and `dev-box` (2) are headed groups carrying the
`SSH` chip above `Scratch` (3); the SSH rows have left Scratch, and counts and
order follow the sessions' first appearance.

The before capture shows the mouse pointer over the terminal pane; the sidebar
region is unaffected.

[Capture metadata](ssh-host-groups-captures.json) records revisions, fixture
options, binary and image hashes, and capture times. The `before` binary was
built from a detached worktree of `f8b9afcd` after
`scripts/ssh-tabs-native-visual.py prepare-before` overlaid this branch's
fixture, exactly as CI does for its Linux artifact. Reproduce either phase
with:

```text
tty7-app --config-dir <temporary-directory> --ssh-tab-visual options.json
{"sidebar": true, "output": "<ignored on Windows>", "hold_seconds": 7200}
```

Wait for `NATIVE_SSH_TABS_READY`, raise the window titled
`SSH tab visual acceptance`, and capture it with Computer Use.

## Automated coverage

`cargo fmt --all -- --check` is clean and `cargo test --locked --workspace`
passes on `efd79855`, the pre-rebase revision (17 suites, 3,203 tests, `cli_e2e` included); the rebased commit `947de8fe` re-ran the sidebar tests and is gated by CI. New tests in
`src/ui/tab_sidebar.rs`: `an_ssh_group_key_round_trips_its_target`,
`sections_place_ssh_groups_between_local_groups_and_scratch` and
`ssh_groups_cannot_be_pinned`.
