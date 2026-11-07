# Source snapshots

The light and dark CSS files and `license` are unchanged copies from
[`sindresorhus/github-markdown-css` v5.9.0](https://github.com/sindresorhus/github-markdown-css/tree/v5.9.0).
The application tests read CSS selectors and declarations from these files and
compare them with the parsed built-in theme; assertions do not maintain a second
handwritten palette.

`../primer/` contains the supporting token definitions from
[`primer/primitives` f48bc063f7bc0fb3e447386a8c259650ce46dea8](https://github.com/primer/primitives/tree/f48bc063f7bc0fb3e447386a8c259650ce46dea8):
selection, background, control and light/dark base colors. Source paths are
recorded in `assets/markdown-themes/GITHUB-NOTICE`. Their MIT license is retained
as `assets/markdown-themes/PRIMER-LICENSE`.

To update the theme:

1. Choose an explicit CSS release and Primer/Octicons commit; update these
   references and the notice before fetching their files.
2. Replace both CSS variants and their license from that release.
3. Run `cargo test --bin tty7-app github` to expose changed source values.
4. Update `assets/markdown-themes/github.yaml` and the renderer mapping only where
   the source changed. Preserve alpha channels and fractional pixel values.
5. Refresh the Primer tokens and icons from the recorded commits if needed.
6. Run the v2 default inheritance and custom-theme switching tests and the visual comparison described
   in `docs/testing/markdown-github-theme.md`.
