# Update downloads with different certificate environments

Some computers fail to download updates with `UnknownIssuer` because their
certificate environment differs from the downloader's bundled roots. The
supplied investigation reports that the same `checksums.txt` download succeeded
with system certificate verification. The affected environment is unavailable
locally, so these checks do not establish that updates now succeed there.

## Cause and scope

The two update paths have different TLS configurations:

```text
Check for a release
  fetch_latest_release -> build_http_client -> ReqwestClient
    native certificate roots by default
    platform verifier with an explicit proxy

Download checksums.txt and the package
  prepare_update -> HttpsFetcher -> ureq
    before: bundled Mozilla roots
    after: operating system certificate verifier
```

A CA trusted by the OS can be absent from Mozilla's roots. This explains why
discovery can work while an asset download fails. `ureq` needs both the
`platform-verifier` feature and an explicit
`RootCerts::PlatformVerifier` configuration. Enabling the feature alone does not
change its default roots.

The downloader is also used to fetch `tty7-server` for remote workspaces, so
those downloads inherit the fix. The feature remains optional; standalone
servers do not gain an HTTPS dependency. Proxy resolution, download limits,
cancellation and SHA-256 checks keep their existing behavior.

The earlier comparison with [cc-switch v4.0.4](https://github.com/farion1231/cc-switch/blob/v4.0.4/src-tauri/tauri.conf.json)
also identifies a separate download mirror. Its two manifest endpoints are
`dl.ccswitch.io` and GitHub. A mirror addresses availability; it is not required
to fix this trust mismatch, and this change adds none.

## Obsolete download errors

`update.json` preserves the last failure across restarts. A manual upgrade has
no updater success outcome to clear an old download failure, so Settings can
show the error even after the target version is installed.

Startup now clears a download failure only when its version parses and is at
or below the running version. The existing record has no typed failure stage,
so this uses the `downloading ` context written by `prepare_update`. Future
versions, unparseable versions, installer failures, unreadable outcomes and
interrupted portable replacements retain their diagnostics.

## Local regression checks

Before the fix, the downloader policy check fails with
`root_certs: WebPki, disable_verification: false`. This is a configuration
regression test, not a reproduction of the affected computer's TLS exchange.
It checks the actual download agent both with and without a configured proxy.

The startup regression test writes a download failure to a scratch
`update.json`, calls the same hydration function used at startup, and checks
both the persisted record and the status shown by Settings. The old code leaves
the obsolete failure in place. A companion test checks that unresolved errors
remain visible.

```powershell
cargo test --locked -p tty7-core --features remote-install daemon::install::
cargo test --locked -p tty7 --bin tty7-app core::update::tests
cargo test --locked --features updater --bin tty7-updater
```

All passed on Windows on 2026-10-09: 130 installer/downloader checks, 34 GUI
update checks, and 17 updater checks. Both previously failing regressions pass.
`cargo fmt --all -- --check` and `git diff --check` also pass. The standalone
Linux musl server dependency tree contains neither `ureq` nor the platform
verifier.

The existing opt-in GitHub test can also exercise the configured TLS backend:

```powershell
cargo test --locked -p tty7-core --features remote-install daemon::install::download::tests::talks_to_github -- --ignored --nocapture
```

It passes after the fix and on the development network before the fix too.
A passing request here does not establish that the affected environment works.

## Affected-machine acceptance still needed

1. Manually install a build containing this fix if the existing downloader is
   blocked. The old binary cannot change its own certificate verifier before
   downloading its replacement.
2. On an affected computer, check for a later official release and confirm that
   both `checksums.txt` and the package download and verify successfully.
3. Confirm that Settings no longer shows a download error for a version that
   is already installed. A failed attempt at a newer version must remain visible.
4. Apply the staged update when local sessions can be stopped, then confirm the
   installed version after relaunch.

The affected computer's certificate configuration, network restrictions and a
complete installation on that machine have not been tested here.
