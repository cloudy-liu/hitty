# UI dependency maintenance

Hitty pins its GPUI component packages to the reviewed revision
`f95e3b89b9a9ec704f9e3e502e4dc0e02cca636e` in `cloudy-liu/gpui-component`.
The authoritative revisions and versions are in `Cargo.toml` and `Cargo.lock`.
All component packages must resolve to the same revision.

Dependency upgrades require a separate maintainer request and validation. A
right-panel or documentation change must not silently update the component
fork, other dependencies or CI actions. Build and test with `--locked`.

When an upgrade is requested, review the source difference, run the component
tests and Hitty's affected application tests, then verify native rendering on
the supported platforms before updating the pin. Preserve third-party license
and source notices. Historical dependency branch names are recorded in
[source provenance](../provenance/source-baseline.md).
