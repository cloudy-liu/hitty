# TextMate color reference

`textmate-colors.json` retains the 23 Atom One Dark 2.3.0 source samples and
220 selected token color expectations in 21 languages. The expected dark colors were produced independently from the bundled
upstream theme files using `vscode-textmate` 9.2.0 and `vscode-oniguruma` 1.7.0
with the installed Cursor language grammars. TOML uses Even Better TOML 0.21.2.
The samples are local test code; they contain no user document contents.

`github-light-colors.json` contains 25 light source samples and 93 selected
token expectations in the same 21 languages. Literal expected colors were
recorded independently from GitHub's default light website tokens captured on
2026-10-06, without reading the generated palette or production mapping script.
The corpus checks representative keywords, constants, strings, functions,
types, variables, tags, markup and the website's gray code-display comment color.
Comment regressions cover Rust line, block and documentation comments,
JavaScript line, block and documentation comments, and Python line comments.
Embedded JavaScript, JSX/TSX and Rust Markdown fences retain separate regressions.

`github-website-colors.json` contains the 70 continuous color ranges of the
reference Rust function from [the GitHub file viewer](https://github.com/cloudy-liu/ctty7/blob/814b4036a05b877a1c3cf3a04e764bbb4822e807/src/main.rs#L35-L59),
captured on 2026-10-06. Expected colors were resolved from the page's actual
`stylingDirectives` CSS classes and the published light stylesheet, independently
of the editor generator. This checks the same source as the user's screenshot,
including parameters, type names, fields, references, constructor calls and bare
variants. The initial mapping failed 37 of these ranges.

The languages are Python, JavaScript, TypeScript, JSON, Rust, CSS, Java, C,
C++, Go, Ruby, Bash, HTML, YAML, Markdown, Diff, SQL, Lua, Make, TOML and TSX.
Ranges identify selected tokens, including declarations, references, builtins,
properties, operators, numbers, strings, comments and markup where relevant.
This is a representative reference corpus, not an exhaustive grammar snapshot.

`editor_theme_matches_authored_color_samples` runs the production
Tree-sitter highlighter and compares the resolved color of every non-whitespace
byte within those ranges against the reference. Separate regressions cover
embedded HTML/JavaScript, JSX, TSX, Rust Markdown fences and the Python import,
variable and magic-variable cases reported in the screenshots.

Other tests compare 11 editor/control colors for each palette directly with
its pinned source, compile all 38 private language queries, and verify that shared
language registrations remain unchanged. No TextMate or Node runtime is
required to build or run tty7 or these tests.

```powershell
cargo test --locked --features updater -j 2 --bin tty7-app editor_theme_
```
