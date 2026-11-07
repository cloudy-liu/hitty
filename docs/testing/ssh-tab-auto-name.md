# SSH tab automatic names

SSH tabs keep their connection identity visible instead of naming every home
directory `~`. Ordinary shell tabs show the connection name at home and
`connection · directory` elsewhere. Task titles and manual names keep their
wording, with the connection in the sidebar's secondary line. Clearing a
manual name resumes automatic naming. Local and WSL tabs retain their existing
rules. Split tabs use the focused pane, or the last focused pane in a background
tab. Exiting an in-pane SSH connection restores local naming.

The connection name comes from the saved profile or SSH alias, falling back to
the connection address. Remote paths must come from the remote shell or remote
workspace; an SSH client's local working directory is not remote evidence.
Unknown paths leave the connection name visible. No remote commands are run to
obtain a name. Automatic disambiguation of identical directory names is deferred.

## Native UI evidence

The opt-in `--ssh-tab-visual` fixture accepts `"auto_names": true` to exercise
these cases with deterministic daemon messages through the production GPUI tab
strip and sidebar. It opens no network SSH connection and does not test remote
Agent detection. Each phase uses identical fixture data and a throwaway config
directory. Windows screenshots are captured with Computer Use
`@oai/sky.get_window_state`, without redrawing application pixels.

| Case | Before | After |
| --- | --- | --- |
| Local terminal | Local terminal | Local terminal |
| SSH home | `~` | `prod-web` |
| Native SSH project | `~/projects/api` | `prod-web · api` |
| Terminal-entered SSH project | `~/projects/api` | `dev-box · api` |
| Remote Codex task | Fix login | Fix login, with dev-box below |
| Manually named SSH tab | Production logs | Production logs, with prod-web below |

| Surface | Before | After |
| --- | --- | --- |
| Sidebar | [JPEG](images/ssh-tab-auto-name/before-sidebar.jpg) | [JPEG](images/ssh-tab-auto-name/after-sidebar.jpg) |
| Top tabs | [JPEG](images/ssh-tab-auto-name/before-top.jpg) | [JPEG](images/ssh-tab-auto-name/after-top.jpg) |

[Capture metadata](ssh-tab-auto-name-captures.json) records the baseline,
fixture options, source and binary hashes, capture times, and image hashes.
The baseline uses the `90bc795c` render paths with the same `auto_names`
fixture overlay. Unused naming helpers in its build do not change those paths.

## Automated coverage

Regression coverage includes home/project names, preserved task and manual
titles, clearing a manual name, SSH exit, focused and remembered split panes,
remote workspace paths, WSL/local identity, mirrored native SSH paths, literal
IPv6, and explicit SSH login/port options including OpenSSH precedence.

An initial parallel workspace run failed the untouched notification sound
settings persistence test. That test passed in isolation. Final validation uses
an isolated build directory and serial execution to avoid shared configuration
and build-artifact interference.

The final Windows run of `cargo test --locked --workspace --features
markdown-visual-tests -- --test-threads=1` passed 3078 tests with zero
failures and 11 existing platform/ignored skips. `cargo check --locked
--all-targets --features markdown-visual-tests`, `cargo fmt --all -- --check`,
and the host-boundary check also passed.

## Integration verification, 2026-10-17

Merged the fork's current main without rewriting the PR branch. Two additional
regressions failed before their fixes: the current-workspace switcher displayed
`/first-pane` for an SSH shell with no known remote path, and a mirrored split
displayed `prod-web` instead of the remembered `staging` connection.

The switcher now suppresses local-directory fallback for SSH and uses the named
pane for local fallback. Mirrored splits containing SSH keep the remembered
pane's title, directory and connection together, including a local pane beside
SSH. Local-only agent naming retains its existing policy.

Windows validation passes 7 tab-view tests, 26 switcher tests and 2 SSH GPUI
tests. These cover unknown and reported SSH paths, SSH exit, two remote splits,
mixed local/SSH splits, WSL and remembered focus. Formatting, diff whitespace
and the HostOps boundary check pass. The earlier native screenshots were not
recaptured for this integration update; cross-platform validation runs in CI.
