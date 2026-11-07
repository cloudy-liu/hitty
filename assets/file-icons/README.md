# File icons

The file tree's file and folder icons are the [Symbols](https://github.com/miguelsolorio/vscode-symbols)
icon theme by Miguel Solorio, vendored unchanged under `symbols/`.

| | |
| --- | --- |
| Version | see `symbols/VERSION` |
| Source | [miguelsolorio/vscode-symbols](https://github.com/miguelsolorio/vscode-symbols) (also the `miguelsolorio.symbols` VS Code extension) |
| License | MIT — see `symbols/LICENSE` |

`build.rs` embeds every SVG under `symbols/files` and `symbols/folders`;
`src/ui/file_icons.rs` reads `symbols/symbol-icon-theme.json` the way VS Code
reads an icon theme (file name, then extensions longest first, then the
default) and rasterises the multi-colour SVGs at the device size.

## Updating

`symbols/` is replaced wholesale, so do not keep local edits in it.

```powershell
git clone --depth 1 --branch <tag> https://github.com/miguelsolorio/vscode-symbols
python scripts/sync-symbols-icons.py vscode-symbols
cargo test -p tty7 --bin tty7-app file_icons
```

An installed copy of the extension works as the source too
(`~/.vscode/extensions/miguelsolorio.symbols-<version>-universal`).
