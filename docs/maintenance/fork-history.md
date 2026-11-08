# Fork maintenance history

Archived from the pre-ctty7 README. Statuses and version names describe that
release line; see the project README for current product capabilities.

## What this fork changes

Everything in this section is a difference from upstream. The **Upstream**
column records where each change stands with the upstream project:

- **Closed upstream** — proposed upstream and closed there, so it is expected
  to stay fork-only.
- **Not submitted** — not yet proposed upstream.
- **Fork-specific** — only meaningful in a fork, so it will not be proposed.
- **Fork default** — the feature exists upstream, but this fork ships a
  different default value.

Changes available only in source builds from `main` are marked **unreleased**.

| Change | Platform | Upstream |
|---|---|---|
| CMD and Cmder prompt reporting, completion, and prompt editing | Windows | Not submitted |
| Coding-agent detection through the Windows process tree | Windows | Not submitted |
| Shift+Enter under ConPTY win32-input-mode | Windows | Not submitted |
| In-pane `ssh` hop detection | Windows | [Closed upstream](https://github.com/l0ng-ai/tty7/pull/739) |
| Vim command-line positioning over in-pane SSH | Windows | Not submitted |
| WSL account login-shell resolution | Windows · WSL | Not submitted |
| Selectable prompt text and Windows path smart-selection | All | Not submitted |
| Native shell history for search and suggestions | All | Not submitted |
| Sidebar group renaming | All | [Closed upstream](https://github.com/l0ng-ai/tty7/pull/735) |
| Agent badges follow the focused pane | All | [Closed upstream](https://github.com/l0ng-ai/tty7/pull/719) |
| Reliable coding-agent identity across restart, failed resume, and exit **(c.6)** | All | Not submitted |
| Exact agent-session restore after server restarts, with shell fallback **(c.7)** | All | Not submitted |
| Restore every open workspace after daemon replacement **(c.8)** | All | Not submitted |
| Transparent, theme-aware avatars for all 19 agents **(c.6)** | All | Not submitted |
| Antigravity brand icon support | All | Not submitted |
| Bell off by default | All | Fork default |
| Update checks without the GitHub REST API | All | Fork-specific |
| Custom `-c` release line and update channel | All | Fork-specific |

One earlier fork change — live focus for split-pane cursors — was accepted
upstream as [l0ng-ai/tty7#736](https://github.com/l0ng-ai/tty7/pull/736), so it
is no longer a difference.

### Windows shell integration

- Adds prompt-boundary and working-directory reports for stock `cmd.exe`, so
  tty7 can provide ghost suggestions, Tab completion, and prompt editing.
- Integrates Cmder through Clink's `CLINK_PATH`, preserving every user-supplied
  argument in launches such as `cmd.exe /K init.bat`.
- Emits the Clink working-directory report outside the prompt string, so a long
  path no longer pushes the prompt itself out of view.
- Uses the Windows process tree when bare CMD cannot report command start, so a
  running full-screen program owns its input line and tty7 re-arms the prompt
  editor only when the command has finished.
- Detects coding agents below CMD, Cmder, wrappers, and helper processes by
  walking the pane's process tree, keeping agent identity on the correct pane
  instead of mistaking prompt helpers or MCP child processes for the foreground
  agent.
- Latches ConPTY's `win32-input-mode` handshake across snapshot replay and
  reconnect and encodes modified Enter as a key event, so **Shift+Enter**
  reaches the shells and agents that ask for it instead of submitting the line.

### Prompt selection and native shell history

- Keeps shell-rendered prompt text selectable and treats Windows drive-letter
  and backslash paths as one smart-selection range.
- Hands Up and Down to the running shell at the edge of a one-line prompt, so
  DOSKEY, PSReadLine, readline, zle, fish, current-session entries, duplicates,
  and custom bindings keep their native behavior.
- Reads PSReadLine and Clink history files for tty7's fuzzy search and ghost
  suggestions without exposing multiline fragments or Clink metadata.

### WSL and SSH

- Resolves the current distro account's login shell through NSS, with
  `/etc/passwd`, inherited `$SHELL`, and `/bin/sh` fallbacks.
- Avoids starting the `wsl.exe --exec sh` bootstrap as the user's shell on WSL
  releases affected by [microsoft/WSL#10718](https://github.com/microsoft/WSL/issues/10718).
- Recognizes an `ssh` session started inside a pane on Windows by walking the
  process tree and reading its arguments, then drops local Git sidebar grouping
  for that pane and refreshes the sidebar once the remote context arrives.
- Keeps Vim's `:wq` echo on its command line when running `ssh` inside a
  Windows pane, as reported in [l0ng-ai/tty7#774](https://github.com/l0ng-ai/tty7/issues/774).
  Removes the cursor-position rewrite from the ConPTY flicker workaround.
  Upstream had already disabled it on macOS and Linux in
  [l0ng-ai/tty7#442](https://github.com/l0ng-ai/tty7/pull/442).

### Sidebar and agents

- Renames repository and sidebar groups from the group header's context menu.
  Submitting an empty name restores the title derived from the path.
- Keeps those custom names in a stable order in the config file, so saving any
  setting does not reshuffle them.
- Makes tab agent badges follow the focused pane in a split.
- Keeps a known coding-agent session id attached to its pane while tty7's
  existing restore flow starts the agent. It keeps this association even if the
  server restarts again before the next hook arrives. Codex and Claude can
  recover an explicit id from a saved resume command. Exiting the agent or
  returning to the shell after a failed resume clears stale identity instead
  of leaving the pane attached to an ended conversation. This hardens tty7's
  resume flow; the agent still owns and stores the conversation. This landed in
  [cloudy-liu/tty7#24](https://github.com/cloudy-liu/tty7/pull/24).
- Saves exact resume targets before relaunching an agent. Codex conversations
  selected through its native history picker can be captured without sending
  a new message. An untouched interactive launch reopens directly, including
  Claude launches whose initial id has no saved history yet. When the original
  conversation cannot be identified, restore leaves a usable shell without
  opening a session picker. Cursor and Antigravity still need an exact id
  already known to tty7. See [agent sessions](../agents/sessions.mdx) and
  [cloudy-liu/tty7#29](https://github.com/cloudy-liu/tty7/pull/29).
- When the background daemon has been replaced, reopens every workspace that
  had a window before shutdown instead of recovering only the last-focused
  project. The foreground workspace opens first; the others return as visible,
  unfocused windows with their saved geometry. Each pane follows the existing
  attach-first path, then resumes its saved agent session when attachment is
  unavailable. A surviving daemon keeps the lighter single-window behavior.
  See [cloudy-liu/tty7#35](https://github.com/cloudy-liu/tty7/pull/35).
- Draws all 19 supported agent marks, including Antigravity, without colored
  circular backgrounds and increases the glyph from 54% to 78% of the avatar.
  Codex, Cursor, and Grok follow the theme foreground; the other agents retain
  their brand hue with brightness correction. Every agent reaches 4.5:1
  contrast on the resting, hover, and selected fills in the sidebar, tab strip,
  and switcher across all 13 built-in themes. This landed in
  [cloudy-liu/tty7#25](https://github.com/cloudy-liu/tty7/pull/25).
- Ships the Antigravity brand mark for agent avatars.

### Defaults that differ from upstream

- **The bell is off** (`"bell": "none"`). Upstream defaults to `"visual"` — a
  flash rather than a sound — so set `"bell"` back to `"visual"` to restore
  upstream's behavior, or to `"audible"` or `"both"` if you want the sound.

### Builds and updates

- Reads the Stable tag from the `github.com` `/releases/latest` redirect and the
  Nightly version from `nightly.json`, rather than the rate-limited REST
  catalog, avoiding the unauthenticated REST catalog's rate limit.

The custom release line described here is historical. See the
[release runbook](release.md) and [migration guide](migration.md) for ctty7's
official release channel.
