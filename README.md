<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/hitty-logo-dark.svg" />
  <img src="assets/hitty-logo-light.svg" alt="Hitty" width="360" />
</picture>

**A terminal workspace for coding agents.**

Persistent sessions, agent status, project files, and remote work in one native app.

[![Release](https://img.shields.io/github/v/release/cloudy-liu/hitty)](https://github.com/cloudy-liu/hitty/releases/latest)
[![CI](https://github.com/cloudy-liu/hitty/actions/workflows/ci.yml/badge.svg)](https://github.com/cloudy-liu/hitty/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

[**Download**](https://github.com/cloudy-liu/hitty/releases/latest) · [Documentation](docs/index.mdx) · [Issues](https://github.com/cloudy-liu/hitty/issues)

**English** · [简体中文](README.zh-CN.md)

</div>

Hitty keeps your shells, coding agents, and project context together. Run an agent,
inspect its changes, read a document, and work on a remote machine without leaving
your workspace. A background server keeps terminal sessions running when the
window closes.

## Quick start

1. **Download a package** from [GitHub Releases](https://github.com/cloudy-liu/hitty/releases/latest).
2. **Open Hitty and choose your shell** in Settings. Windows supports PowerShell, CMD, and a custom Cmder/Clink startup command.
3. **Open your project and start an agent** such as `codex`, `claude`, or `gemini`. Use tabs and split panes for parallel work; open files alongside the terminal to review the result.

| Platform | Package | Run |
| --- | --- | --- |
| Windows x64 | `hitty-*-windows-x86_64-setup.exe` / `hitty-*-windows-x86_64.zip` | Install normally, or extract the entire ZIP and open `hitty-app.exe`. |
| macOS Apple Silicon | `hitty-*-macos-arm64.dmg` | Drag **Hitty** into Applications. |
| macOS Intel | `hitty-*-macos-x86_64.dmg` | Drag **Hitty** into Applications. |
| Linux x64 | `hitty-*-linux-x86_64.AppImage` | Make it executable, then run it. A `.tar.gz` archive is also available. |

Windows packages are unsigned. macOS packages are ad-hoc signed and are not
notarized, so the operating system may block the first launch. Linux archive
builds need the system libraries listed in the [installation guide](docs/getting-started/installation.mdx).
Every release includes `checksums.txt`.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/screenshots/hitty/workspace-dark.png" />
  <img src="assets/screenshots/hitty/workspace-light.png" alt="Hitty workspace with agent status, project groups, document tabs, Markdown preview, and a file tree" width="100%" />
</picture>

*Native Windows captures of Hitty. Agent states use controlled sample events;
no live agent request is made. [Capture details](assets/screenshots/hitty/README.md).*

## Core capabilities

### Sessions that outlive the window

A background server owns your shells and running processes. Close and reopen the
window to reattach to the same sessions. After a server or machine restart,
Hitty restores saved workspaces and scrollback and can resume supported agent
conversations when their session IDs are known. A reboot does not keep the
original shell processes alive.

Organize work with tabs, split panes, project groups, pinned groups, and remembered
layouts. [Workspaces and panes](docs/getting-started/concepts.mdx) · [Tabs and splits](docs/window/tabs-and-splits.mdx)

### Know which agent needs you

Recognize coding agents including Claude Code, Codex, Gemini, Cursor, and OpenCode.
Per-pane status distinguishes working, waiting for input, idle, and unread results.
Desktop notifications, tray attention, and sounds help you notice completed work
without watching every terminal.

Send a selection or repository diff to the active agent from the command palette.
Use the CLI to inspect agent status or wait for a state change. Available status
and conversation-resume details depend on the agent and its integration.
[Agent status](docs/agents/status.mdx) · [Session restore](docs/agents/sessions.mdx)

### A terminal that works with your shell

Get multiline prompt editing, history suggestions, completion, searchable
scrollback, and file and URL links. Shell integration supports PowerShell,
bash, zsh, fish, and Windows CMD/Clink, including Cmder startup scripts.
PowerShell and Clink history remain available through history search.

Choose the shell, arguments, fonts, theme, and shortcuts that fit your workflow.
[Shell integration](docs/reference/shell-integration.mdx) · [Prompt editing](docs/terminal/prompt.mdx)

### Read, edit, and review beside your terminal

Keep several files open in document tabs, with separate edits and preview modes.
Markdown preview includes Mermaid diagrams, zoom and pan, code copying, front
matter, and custom reading themes. Local HTML previews use WebView2 on Windows
and WKWebView on macOS; Linux uses the source view.

Browse files, search source, review Git changes in split or unified diffs, inspect
commit history, and manage worktrees. Expand a document to fill the workspace and
restore it when you return to the terminal.
[Markdown themes](docs/customization/markdown-themes.mdx) · [Git review](docs/git/source-control.mdx)

<table>
  <tr>
    <td><img src="assets/screenshots/hitty/editor-light.png" alt="Hitty source editor with file tabs, breadcrumbs, and syntax highlighting" width="100%" /></td>
    <td><img src="assets/screenshots/hitty/html-preview-light.png" alt="Hitty rendering a local HTML document beside its project files" width="100%" /></td>
  </tr>
  <tr>
    <td align="center">Source editor</td>
    <td align="center">Local HTML preview</td>
  </tr>
</table>

### Local, SSH, and WSL workspaces

Connect through saved SSH profiles with key authentication, jump hosts, SFTP,
and port forwarding. Remote workspaces keep terminals, files, and Git operations
on the machine that owns them. Reconnect to a running remote server without
recreating its sessions.

On Windows, connect to WSL distributions using the bundled Linux server. Choose
SSH and WSL connections in the desktop interface; the CLI can address established
remote links. [SSH](docs/remote/ssh.mdx) · [Remote workspaces](docs/remote/workspaces.mdx)

### Scriptable from the command line

The bundled `hitty` command lets scripts and agents inspect workspaces, run
commands, send input, and read terminal output. Start Hitty once, then try:

```sh
hitty doctor                 # inspect the installation and server
hitty ls                     # list workspaces
hitty agents                 # show agent states
hitty run -- cargo test      # stream output and return the command's exit code
hitty pane ls --json          # get pane addresses for scripts
```

Use a pane ID returned by `hitty pane ls` with `hitty send`, `hitty capture`,
or `hitty wait`. Add `--json` for machine-readable output.
[CLI guide](docs/cli/overview.mdx) · [Command reference](docs/cli/reference.mdx)

## Make it yours

Most preferences are available in Settings. Default terminal typography is
**Hack, 15 px, 1.4 line height**, with a light theme. On Windows, the default shell
is PowerShell 7 when available, otherwise Windows PowerShell. Cmder is configured
as `cmd.exe` with your own `/k` initialization script.

| Platform | Configuration |
| --- | --- |
| Windows | `%APPDATA%\hitty\config.json` |
| macOS / Linux | `~/.config/hitty/config.json` |

Use `HITTY_CONFIG_DIR` or `--config-dir` for a separate configuration.
[Settings](docs/customization/settings.mdx) · [Fonts](docs/customization/fonts.mdx) · [Themes](docs/customization/themes.mdx)

## Build and contribute

Hitty is written in Rust with a GPU-rendered native interface. Use stable Rust,
MSVC C++ tools and the Windows SDK on Windows, or Xcode command line tools on
macOS. Linux needs the [native development libraries](docs/getting-started/installation.mdx#building-from-source).

```sh
git clone https://github.com/cloudy-liu/hitty.git
cd hitty
cargo build --locked --workspace --bins --features updater
cargo dev
```

`cargo dev` uses `.hitty-dev` for isolated application settings. Run
`cargo fmt --all -- --check` and `cargo test --locked --workspace --features updater`
before submitting code changes. On Windows, the full test suite also needs
Git for Windows' `usr/bin` utilities on the test process's PATH.

Report bugs and propose changes in [GitHub Issues](https://github.com/cloudy-liu/hitty/issues).
Pull requests target this repository's `main` branch. See [contributor guidance](AGENTS.md).

## Credits and license

Hitty is distributed under the [Apache License 2.0](LICENSE). It began from
[ctty7](https://github.com/cloudy-liu/ctty7), which was developed from
[tty7](https://github.com/l0ng-ai/tty7). Original copyright and attribution notices
are retained; the exact source revision and import details are recorded in
[source provenance](docs/provenance/source-baseline.md).

Thanks to the projects and contributors behind the terminal, interface, and assets:

- [GPUI / Zed](https://github.com/zed-industries/zed), [gpui-component](https://github.com/longbridge/gpui-component), and [Alacritty](https://github.com/alacritty/alacritty) for core building blocks.
- [Herdr](https://github.com/herdrdev/herdr) for the Apache-2.0 agent screen-detection manifests; [provenance](assets/agent-detection/README.md).
- Miguel Solorio's [Symbols](https://github.com/miguelsolorio/vscode-symbols) for file icons; [MIT license](assets/file-icons/symbols/LICENSE).
- [Hack](https://sourcefoundry.org/hack/) for the bundled font; [font licenses](assets/fonts/hack/LICENSE.md).
- GitHub, Primer, and Atom One Dark contributors for reading and editor styles; [Markdown notices](assets/markdown-themes/GITHUB-NOTICE) and [editor theme licenses](assets/editor-themes/README.md).
- Microsoft for the Windows ConPTY components; [MIT license](assets/windows/conpty/LICENSE.txt).

Third-party components and assets retain their respective licenses and notices.
