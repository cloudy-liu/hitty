# Update cancellation and failure recovery

This follows the updater inspection associated with cloudy-liu/ctty7#124.
That PR handles certificate environment differences on some computers.
This change handles waiting, cancellation, and cleanup.

## Requirements

- Bound the wait for the GUI to exit. A timeout must leave installed files
  alone and must not start a second GUI beside the one still running.
- Bound verification commands, the portable daemon-stop helper, Windows
  Setup, and the elevated install stage. Stop their process trees before
  cleanup or recovery. If termination cannot be confirmed, retain the
  staging files, record the failure, and prevent an automatic relaunch.
- Cancel desktop update requests while waiting for the connection, TLS,
  response headers, checksums, or package bytes. Retain proxy selection,
  certificate verification, checksum verification, and size limits.
- Preserve the previous macOS bundle or Linux AppImage when replacement and
  restoration both fail. A successful restoration must report the original
  failure before relaunching the previous app.
- Keep recovery files during immediate cleanup and later orphan-stage
  sweeps. Retry ordinary Windows stage deletion after the updater exits.
- Preserve installation consent and the existing app/daemon/server scope.
  Windows stops the local daemon; WSL/SSH servers follow their separate
  connection and protocol checks.

## Behavior

| Wait | Limit |
| --- | --- |
| GUI exit | 120 seconds |
| Installed updater capability probe | 5 seconds |
| Verification subprocess or Unix verification utility | 2 minutes |
| Portable daemon-stop subprocess | 45 seconds |
| Windows Setup process tree | 30 minutes |
| Elevated install-stage process tree | 35 minutes |
| Confirming process termination | 5 seconds |
| Retrying ordinary Windows stage cleanup | 5 seconds |

Desktop downloads use the HTTP runtime already used for update checks.
Each file retains a 15-minute budget and a 30-second connection limit.
Cancellation is polled every 100 milliseconds and drops the active request
or response. The existing proxy resolver selects manual, system, then
environment settings and applies bypass rules to redirects. TLS uses the
existing platform verifier. Packages retain the 128 MiB limit; the checksum
manifest is limited to 1 MiB.

Each request and redirect applies the selected proxy's bypass rules before
resolving it. SOCKS proxy resolution has a cancellable 30-second limit. An
unavailable required proxy stops the download; an unused bypassed proxy
cannot block a direct download. Redirects have a ten-hop limit.

Verification subprocesses receive the same cancellation predicate. They run
in Windows jobs or Unix process groups, with stdout and stderr redirected
to temporary files. This avoids waiting indefinitely for inherited pipes.
Unix re-verification retains process failures through version and signature
checks, so unconfirmed termination also prevents recovery relaunches.

Ordinary stages can be removed. A stage containing a previous app/image or
an unconfirmed-process marker is retained for recovery, including after the
24-hour orphan-stage cutoff. A failed restoration reports its backup path.
Windows cleanup waits for the staged updater to exit and retries transient
deletion failures. The next startup can sweep ordinary leftovers.

Recovery markers include the process ID, failure detail, and stage path.
Desktop verification records the failure in its stage and application log
even if cancellation has superseded the attempt. It leaves a newer attempt's
state and installation consent alone. The Windows relaunch watcher checks
the attempt's recovery marker as well as the update guard; an inaccessible
process or a stale guard PID cannot authorize a recovery relaunch.

## Before and after evidence

The following tests were run against the original operations before the
behavior changed. Download and replacement operations were first extracted
without changing their behavior so the tests could call those paths.

- Before, all three initial cancellation tests failed: an already-cancelled
  attempt still contacted the server, and both stalled-checksum cases waited
  for the peer to close and returned a connection error.
- Before, a replacement followed by a failed restoration deleted the only
  previous version. Another test restored the old file but found that no
  relaunch occurred.
- After, those cases are covered by the same regression tests. Additional
  tests cover cancellation during TLS and package reads, closed connections,
  download limits/progress, process-tree termination, parent deadlines,
  preservation during sweeps, and failure cleanup.
- Review regressions also failed before their fixes: an inaccessible live
  holder was treated as exited; unconfirmed verification and a stale guard
  still allowed relaunch; a cancelled verifier's record lacked its failure;
  and an unresolved SOCKS proxy caused a direct request. Their tests now
  cover conservative process observation, recovery records, and proxy failure.
- A follow-up regression caught resolution of an unused bypassed proxy.
  Coverage includes bypassed downloads, redirects in both routing directions,
  and relative redirect loops.

Run the desktop update and updater suites:

    cargo test --locked --features updater --bin tty7-app core::update
    cargo test --locked --features updater --bin tty7-updater
    cargo test --locked -p tty7-core daemon::update_guard::tests

The ignored process-fixture tests are executed as children by the surrounding
tests. They are not skipped acceptance tests.

The access-denied fixture disables privileges on its own impersonating
thread, so elevated CI runners also exercise the denied-handle path without
changing the process token shared by other tests.

## Verification limits

The local fixtures use loopback HTTP, temporary installations, and dedicated
child processes. They do not reproduce the affected computer's certificate
environment. They do not perform a real Setup upgrade, answer UAC, or stop a
user's running app or daemon.

The shared Unix process, replacement, and cleanup code can also run against
Linux through a source-import test harness. Full platform builds and the
macOS signed-binary tests run in CI.

Windows Setup still has no application-owned complete rollback. Stopping a
timed-out installer may leave a partial installation requiring manual Setup.
Portable/macOS/AppImage recovery preserves prior files, but it cannot
resurrect shell or agent processes already stopped by an update. Filesystem
operations and OS termination remain subject to the operating system;
unconfirmed termination is reported instead of claiming cleanup succeeded.
