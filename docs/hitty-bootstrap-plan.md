# Hitty bootstrap decisions

This document records confirmed decisions from the continued design discussion; it does not report implementation completion. Where it differs from cloudy-liu/hitty issue #1, the newer decisions below supersede the earlier specification.

## Confirmed

- Start from a reviewed ctty7 source snapshot with independent Git history and project records, as recorded in [ADR 0001](adr/0001-independent-source-snapshot.md).
- Keep the existing README bootstrap commit and import application source in a separate baseline commit.
- Use the Hitty product name and the existing double-pane icon with the selected Hitty wordmark.
- Separate the source baseline, branding, and release work from larger architectural refactors.
- **Q1 — First version:** use `v0.1.0`, then advance versions within Hitty's own release line.
- **Q2 — First release gate:** release Windows, macOS, and Linux together only after all three platforms pass the applicable acceptance checks. A Windows-only preview is not the selected first-release plan.
- **Q3 — Fresh installation:** ctty7 has only been used internally by the maintainer, so Hitty does not need a legacy-user transition. Start with Hitty's own fresh user data. Do not implement legacy configuration detection, an import UI, settings or credential migration, conflict merging, or live-session adoption. Special compatibility for simultaneous use of the old and new products is outside this release's scope.
- **Default settings:** retain the existing source-code defaults. The suggestion to turn the maintainer's personal configuration into product defaults was explicitly withdrawn; the maintainer will customize settings after installation. No personal configuration snapshot should be shipped or applied.
- **Application identity:** fully rename application-owned source identifiers and product identities to Hitty, including package/crate and binary names, application types, commands, environment variables, configuration and runtime paths, integration assets, and build/release targets. Use `Hitty` for the product, `hitty` for lowercase names, `HITTY` for uppercase prefixes, and `Hitty` in type names. Do not leave old brand names in active application-owned identities or add compatibility aliases for them. This changes product identity while preserving functional behavior and user-facing preference defaults.
- **Q4 — Source revision:** use ctty7 revision `7f430a8f5f89a3a2963250b196b0baea02569299` as the source snapshot. Perform subsequent rebranding and fixes in Hitty. Diagnose the two previously observed macOS Intel CI failures separately and resolve them before the three-platform first-release gate; their cause is not yet established.

## Rename scope

The following mapping makes the requested full application rename concrete. The verified GUI binary is `tty7-app`, not `tty-app`.

| Existing application-owned identity | Hitty identity |
| --- | --- |
| Product display name `ctty7` | `Hitty` |
| Root package and CLI `tty7` | `hitty` |
| `tty7-app`, `tty7-updater` | `hitty-app`, `hitty-updater` |
| `tty7-core`, `tty7-cli`, `tty7-server` | `hitty-core`, `hitty-cli`, `hitty-server` |
| Rust names such as `tty7_core`, `Tty7App` | `hitty_core`, `HittyApp` |
| `TTY7` and `TTY7_*` environment variables | `HITTY` and `HITTY_*` |
| Product configuration/data paths, keyring services, socket prefixes, hooks, and remote-server assets | Corresponding `hitty` names |
| App bundles, desktop integration, installer metadata, and release assets | Corresponding Hitty identities, including a newly generated Windows installer GUID |
| Update and application/server download destinations | `cloudy-liu/hitty` releases |

Apply the rename across owned filenames, source, scripts, build configuration, tests, generated integration templates, documentation, and packaged artifacts. Update the GUI, CLI, daemon, remote server, updater, and hooks together. Preserve shared technical names such as `daemon.sock` where the enclosing Hitty directory already supplies the product identity.

Retain ordinary terminal terminology and external contracts: for example, keep the `TERM_PROGRAM` variable name while changing its product value to `hitty`; keep `portable-pty`, OSC notification codes, and protocol/control version numbers. The product release reset to `0.1.0` does not reset protocol versions.

Retain existing third-party dependency URLs and pinned revisions. The inspected manifest and lockfile have no old product names in their actual external Git source URLs. Historical comments referring to a real upstream `tty7` branch must be rewritten accurately or moved to provenance notes, never relabeled as an invented `hitty` branch. Existing license, copyright, and source attribution material stays truthful; old product names may remain in those provenance records.

Remove old-product data adoption and old-GUI installer cleanup behavior. Do not rename those paths into a fictitious Hitty migration history or add old-brand aliases. The separate baseline commit records the imported source as it was; the completed rebranding tree and packages must use Hitty's active identities.

## Implementation checks

- Compare the source import with the selected revision and account for any intentional differences. Preserve the selected uncommitted branding assets separately from the source baseline.
- Scan owned paths and contents for old product identifiers after rebranding, and account for every retained historical or third-party occurrence. Verify installed identities and packaged names as well as source strings.
- Diagnose and resolve the previously observed macOS Intel failures in Markdown double-click selection and SCM back-navigation focus. Do not assume these are harmless flakes.
- Validate the existing four desktop targets across the three platforms: Windows x86_64, Linux x86_64, macOS ARM64, and macOS x86_64, together with required remote-server artifacts.
- Check clean first-run behavior with the retained preference defaults, normal terminal/Agent/SSH/WSL behavior, installation, and updates between Hitty versions. Assemble and verify the first release as a draft before publication.

The maintainer confirmed the complete scope (Q5). GitHub issue #1 is synchronized with the specification in `docs/specs/0001-hitty-bootstrap.md`. The source baseline is recorded in commit `4699e7e`; rebranding and validation are in progress. No Hitty release has been published. Preference defaults and the maintainer's current configuration remain unchanged.
