## Problem Statement

Hitty needs an independent product identity, repository history, and release line while retaining the working capabilities of the existing ctty7 implementation. The source currently mixes ctty7 display branding with tty7 code, command, configuration, integration, packaging, and update identities. A source copy alone would not produce a complete Hitty application.

The previous application was used internally by its maintainer. There is no external legacy-user migration requirement. Hitty should start as a fresh installation with the existing source-code preference defaults.

## Solution

Import the selected ctty7 source snapshot into cloudy-liu/hitty without its Git ancestry, branches, tags, issues, pull requests, or releases. Preserve Hitty's existing README bootstrap commit. Record the first application source baseline separately from subsequent changes, with its exact source revision and original license and attribution material.

Fully rename application-owned code and runtime/distribution identities to Hitty. Retain the existing double-pane icon and the selected Hitty wordmark direction. Preserve functional behavior and source-code preference defaults. Remove old-product migration behavior and aliases. Release v0.1.0 only after Windows, macOS, and Linux pass the applicable acceptance checks together.

## User Stories

1. As a maintainer, I want independent Hitty history and project records so that future work has a clear home.
2. As a contributor, I want an exact source revision so that I can investigate inherited behavior.
3. As a reviewer, I want source import separated from rebranding so that I can understand each change.
4. As a contributor, I want the original licenses and attribution retained so that source provenance remains accurate.
5. As a maintainer, I want the selected branding assets preserved so that the approved direction is not lost.
6. As a user, I want the product consistently named Hitty so that I can identify it across desktop surfaces.
7. As a user, I want the retained application icon paired with the Hitty name so that the selected visual identity is implemented.
8. As a developer, I want application-owned packages, types, filenames, and scripts to use Hitty names so that the implementation has one product identity.
9. As a command-line user, I want the hitty command and HITTY configuration overrides so that scripts address the new product.
10. As a new user, I want a fresh installation with the existing preference defaults so that I can customize it normally.
11. As a new user, I want an unobstructed first launch so that no old-product import flow interrupts setup.
12. As a terminal user, I want shells, panes, workspaces, and persistence to remain functional after renaming.
13. As a coding-agent user, I want hooks, detection, and notifications to address Hitty correctly.
14. As an SSH user, I want saved Hitty connections and credentials to use Hitty's own identities.
15. As an SSH or WSL user, I want compatible Hitty remote services and assets so that remote workspaces function.
16. As a user, I want installers and operating-system identities to describe Hitty as its own product.
17. As a user, I want downloads and updates to resolve Hitty releases and matching assets.
18. As an installed user, I want subsequent Hitty updates to preserve a recoverable application when an update fails.
19. As a release maintainer, I want complete assets and checksums verified in a draft before publication.
20. As a user on any supported platform, I want the first release to meet the agreed three-platform acceptance gate.
21. As a reviewer, I want real application screenshots and reproducible checks so that I can verify the rename.
22. As a future contributor, I want a usable baseline and Hitty-specific project guidance before larger refactoring begins.

## Implementation Decisions

- **Destination:** cloudy-liu/hitty is the repository, tracker, PR destination, and application/server release source. Keep the ready-for-agent specification label. The original checkout and repository remain the historical reference.
- **Source revision:** import exactly 7f430a8f5f89a3a2963250b196b0baea02569299. Export tracked source without old Git metadata or local runtime/build data. Account for export attributes, submodules, and large-file pointers if present.
- **Traceability:** preserve the README bootstrap history, record source provenance, and separate the baseline from rebranding. Document intentional bootstrap metadata differences. Keep release publication inactive until destinations have been reviewed.
- **Brand:** use Hitty for display names, hitty for lowercase identities, HITTY for environment prefixes, and Hitty in type names. Retain the existing application/tray artwork and selected wordmark direction.
- **Rename coverage:** include packages, crates, binaries, modules/types, owned filenames, tests, scripts, manifests, help text, configuration/runtime paths, credential services, hooks, remote services, installer metadata, shortcuts, application IDs, release assets, and update URLs. Generate a fresh Windows installer GUID. Do not retain old-brand aliases.
- **Meaningful exceptions:** preserve ordinary TTY/PTY terminology, external protocol keys, third-party package names, pinned dependency URLs/revisions, and truthful license/copyright/provenance material. Product-valued protocol fields use Hitty. Product version reset does not reset protocol/control version counters. Move or accurately rewrite historical source comments rather than inventing renamed upstream branches.
- **Fresh installation:** no old-product detection, import UI, configuration or credential migration, conflict merge, live-session adoption, or special dual-product compatibility layer. Remove inherited old-product data adoption and obsolete installer cleanup instead of renaming them into fictitious Hitty history.
- **Preferences:** retain source-code defaults. Do not seed the maintainer's personal settings, connections, paths, credentials, or runtime state. Users customize their new installation normally.
- **Runtime:** coordinate GUI, CLI, daemon, updater, hooks, SSH, and WSL identities. Use Hitty-owned writable state and discovery paths.
- **Architecture:** retain the existing Rust workspace and external framework/component dependencies. Keep larger architectural refactors and unrelated changes separate.
- **Version and platforms:** the first release is v0.1.0. Validate Windows x86_64, Linux x86_64, macOS ARM64, and macOS x86_64, plus required remote-server artifacts. Publish the three desktop platforms together after acceptance; there is no Windows-only preview in this plan.
- **Inherited failures:** diagnose the two observed macOS Intel CI failures in Markdown double-click selection and SCM back-navigation focus. Fix them in Hitty before first-release acceptance; their cause is not assumed to be test flakiness.
- **Release:** assemble a draft, verify assets/checksums, then publish explicitly only when acceptance is complete. Write English user-facing release notes. The initial release records its source baseline; subsequent release comparisons use Hitty tags.
- **Guidance:** establish Hitty-specific agent/contribution guidance and squash merging. Retain real before/after application screenshot evidence for visual changes.

## Testing Decisions

- **Primary seam:** use the existing public application, CLI, installation, daemon, remote-service, and update boundaries with isolated user data. Reuse existing tests and native smoke/visual harnesses.
- **Source import:** prove the new history descends from the Hitty bootstrap without old ancestry or imported tags/branches. Compare the imported source against the selected revision and explain each deliberate difference.
- **Rename acceptance:** scan owned paths/content and packaged artifacts for old product identities; account for retained historical/third-party occurrences. Test resulting behavior rather than string-replacement implementation.
- **Functional regression:** run applicable locked builds and workspace/updater tests. Exercise local shell input/output, panes/workspaces, session reconnect, CLI, agent detection/notifications, and remote services.
- **Fresh install:** verify first-run defaults and Hitty-owned state without loading old-product data. Legacy-import and merge scenarios are out of scope.
- **Integration:** verify hooks and credential identities, compatible SSH/WSL services, remote asset selection, and discovery. A local build alone is not proof of remote behavior.
- **Updater/release:** inspect packaged executable layouts, asset names, download destinations, and checksums. Test between Hitty versions and retain existing staged-update, recovery, and failure coverage. Do not publish a public release merely to test the updater.
- **Visual evidence:** capture the baseline and Hitty from the same reproducible state, viewport, scale, and settings. Inspect real application and relevant installation/system surfaces. Keep durable images and link them directly in PR evidence.
- **Platform reporting:** complete the three-platform gate before first publication. State unavailable runtime validation precisely. Keep affected visual acceptance unverified and PRs draft if required captures are blocked, unless an explicit limitation is accepted.

## Out of Scope

- Importing old Git ancestry, author-history metadata, branches, tags, issues, PRs, or releases.
- Renaming, rewriting, archiving, or deleting the old repository or modifying external dependency repositories.
- Removing original licenses, copyright, or truthful attribution.
- Legacy-user migration, old-brand aliases, old-product upgrade compatibility, or live-process adoption.
- Turning the maintainer's personal configuration into product defaults.
- Terminal-engine rewrites, framework replacement, broad redesign, or new logo exploration.
- Publishing before complete release assets and acceptance evidence are ready.

## Further Notes

The maintainer confirmed this scope after selecting the source revision, full internal/product rename, fresh installation, unchanged preference defaults, v0.1.0, and a joint three-platform first release. This specification supersedes its earlier legacy-data import requirements. Source import, rebranding, and verification are implementation work; specification approval alone does not mean a release exists.
