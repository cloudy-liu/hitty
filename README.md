# Hitty

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/hitty-logo-dark.svg">
  <img src="assets/hitty-logo-light.svg" width="440" alt="Hitty">
</picture>

A terminal workspace for coding agents.

Hitty is starting as an independent project based on a source snapshot of [ctty7](https://github.com/cloudy-liu/ctty7), which originated from [tty7](https://github.com/l0ng-ai/tty7).

## Project status

The selected application source has been imported as a separate baseline. Application-owned code, runtime paths, installers, and branding now use Hitty. First-release validation is in progress, and no Hitty release is available yet.

The first release will ship Windows, macOS, and Linux together after acceptance. See the [verification record](docs/testing/hitty-bootstrap.md), [release procedure](docs/maintenance/hitty-release.md), and [source provenance](docs/provenance/source-baseline.md).

## A fresh start

- Hitty has its own Git history, issues, pull requests, and releases.
- The source import does not include ctty7's existing Git commits, branches, or tags.
- Earlier implementation history, discussions, and releases remain available in the ctty7 repository.
- The import records its exact source revision and retains applicable licenses, copyright notices, and attribution.
- The initial source baseline is recorded separately from subsequent rebranding and larger refactors.

Implementation planning is tracked in [GitHub Issues](https://github.com/cloudy-liu/hitty/issues).

## Build and run

Use a stable Rust toolchain and the platform development tools. Linux also needs
the native libraries listed in `.github/workflows/ci.yml`.

```sh
cargo build --locked --workspace
cargo dev
cargo test --locked --workspace
cargo test --locked --features updater --bin hitty-updater
```

`cargo dev` uses `.hitty-dev` for its configuration. The GUI binary is
`hitty-app`; the public CLI is `hitty`. See [CLI usage](docs/cli/overview.mdx) and
[contributor guidance](AGENTS.md).
