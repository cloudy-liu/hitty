"""Regenerate Atom One Dark and GitHub Light from pinned MIT sources.

Run from the repository root. TextMate scopes are selected explicitly: rule
names are not identifiers and can be duplicated in the upstream theme.
The editor-only queries namespace captures so injected languages keep their
own overrides instead of inheriting the containing document's colors.
"""

import json
import hashlib
from pathlib import Path
from urllib.request import urlopen

DEST = Path(__file__).resolve().parent.parent / "assets" / "editor-themes"
SOURCES = {
    "dark": ("onedark", "a8be970644982221f9b61fb1c4b3da74b4beab79", "OneDark"),
}
FINGERPRINTS = {
    "dark": "53dbe439275f8b382dd9aabc1cc72a5447ed71b8aa47ab370b5dde50e795df7b",
    "light": "24242363bfd0f481fb736da907d1215165c9eac3d985bf8f9809843c88e7d349",
}

# These are Tree-sitter roles, paired with authored TextMate scopes.
ROLES = {
    "keyword": "keyword", "operator": "keyword.operator",
    "function": "entity.name.function", "function.builtin": "support.function",
    "function.method.builtin": "support.function", "constructor": "entity.name.function",
    "type": "entity.name.type", "type.builtin": "support.type",
    "enum": "entity.name.type", "variant": "entity.name.type",
    "variable": "variable", "variable.special": "variable",
    "variable.builtin": "variable", "variable.member": "variable",
    "variable.parameter": "variable.parameter", "property": "variable",
    "constant": "constant", "number": "constant.numeric", "boolean": "constant",
    "string": "string", "string.escape": "constant.character.escape",
    "escape": "constant.character.escape", "string.regex": "string.regexp",
    "string.special.regex": "string.regexp", "string.special": "string",
    "string.special.symbol": "constant.other.symbol", "comment": "comment",
    "comment.doc": "comment", "tag": "entity.name.tag", "tag.doctype": "keyword",
    "attribute": "entity.other.attribute-name", "preproc": "keyword", "label": "constant",
    "link_text": "entity.name.function", "link_uri": "string.other.link",
    "title": "entity.name.section", "text.literal": "string", "text.code.span": "string",
    "embedded": "variable.interpolation", "emphasis": "markup.italic",
    "emphasis.strong": "markup.bold", "primary": "$foreground",
    "punctuation": "$foreground", "namespace": "$foreground", "module": "$foreground",
    # Capture vocabulary used by the bundled Scala, SQL, Make and Zig queries.
    "conditional": "keyword", "exception": "keyword", "include": "keyword",
    "repeat": "keyword", "storageclass": "storage", "float": "constant.numeric",
    "character": "string", "parameter": "variable.parameter", "field": "variable",
    "method": "entity.name.function", "import": "keyword", "cImport": "keyword",
    "delimiter": "$foreground", "text.uri": "string.other.link",
    "text.reference": "entity.name.function", "none": "$foreground",
}
OVERRIDES = {
    "python": {
        "variable": "$foreground", "constant": "source.python constant.other",
        "constant.builtin": "source.python constant", "property": "$foreground",
        "constructor": "$foreground", "variable.parameter": "source.python variable.parameter",
        "variable.builtin": "support.variable.magic.python", "type.parameter": "meta.function.parameters.python",
    },
    "javascript": {
        "variable": "variable.other.readwrite.js", "variable.parameter": "variable.parameter",
        "operator": "source.js keyword.operator", "keyword.operator.word": "keyword",
        "punctuation.key_value": "punctuation.separator.key-value.js",
        "variable.constant": "variable.other.constant.js", "variable.builtin": "source.js support.variable",
        "string.special": "string.regexp", "keyword.default": "keyword.control.default.js",
    },
    "typescript": {
        "variable": "variable.other.readwrite.ts", "variable.parameter": "variable.parameter",
        "operator": "source.ts keyword.operator", "keyword.operator.word": "keyword",
        "punctuation.key_value": "punctuation.separator.key-value.ts",
        "variable.constant": "variable.other.constant.ts", "variable.builtin": "source.ts support.variable",
        "string.special": "string.regexp", "keyword.default": "keyword.control.default.ts",
    },
    "json": {"boolean": "constant.language.json", "constant.builtin": "constant.language.json"},
    "rust": {
        "type": "entity.name.type.rust", "type.builtin": "storage.type.core.rust",
        "variable.parameter": "variable", "label": "entity.name.lifetime.rust",
        "attribute": "meta.attribute.rust",
    },
    "css": {
        "property": "support.type.property-name", "type": "keyword.other.unit",
        "string.special": "constant.other.color", "attribute.id": "entity.other.attribute-name.id",
        "constant.color": "support.constant", "constant.builtin": "support.constant.property-value.css",
    },
    "java": {"type.builtin": "storage.type.primitive", "namespace": "source.java storage.modifier.import", "variable.argument": "$foreground"},
    "c": {"variable": "$foreground", "property": "$foreground", "type.builtin": "storage.type.primitive", "operator": "source.c keyword.operator"},
    "cpp": {"variable": "$foreground", "property": "$foreground", "type.builtin": "storage.type.primitive", "operator": "source.cpp keyword.operator"},
    "ruby": {"constructor": "entity.name.type", "variable": "$foreground", "variable.definition": "variable"},
    "go": {"namespace": "entity.name.type"},
    "toml": {"type": "support.type.property-name", "property": "support.type.property-name", "operator": "$foreground"},
    "make": {"string": "$foreground", "operator": "$foreground", "constant": "variable", "constant.macro": "entity.name.function"},
    "markdown": {"title": "entity.name.section.markdown", "title.setext": "markup.heading.setext", "punctuation.heading": "punctuation.definition.heading.markdown", "text.literal": "markup.raw.block.markdown"},
    "markdown_inline": {"link_uri": "markup.underline.link.markdown"},
    "elixir": {
        "constant": "source.elixir constant.language", "constant.builtin": "source.elixir constant.language",
        "number": "source.elixir constant.numeric", "operator": "source.elixir keyword.operator",
        "module": "entity.name.type", "variable.definition": "source.elixir variable.definition",
    },
    "sql": {"variable": "$foreground", "field": "$foreground", "parameter": "$foreground", "type": "$foreground"},
    "diff": {"constant": "$foreground", "attribute": "$foreground", "keyword": "$foreground",
             "string": "$foreground", "variable.builtin": "$foreground",
             "markup.inserted": "markup.inserted", "markup.deleted": "markup.deleted"},
}
OVERRIDES["tsx"] = OVERRIDES["typescript"]


def scopes(rule):
    value = rule.get("scope", [])
    return [s.strip() for s in value.split(",")] if isinstance(value, str) else value


def theme_style(data, scope):
    if scope == "$foreground":
        return {"color": data["colors"]["editor.foreground"]}
    matches = [r["settings"] for r in data["tokenColors"] if scope in scopes(r)]
    assert matches, scope
    value = matches[-1]
    style = {"color": value["foreground"]}
    if value.get("fontStyle") == "italic":
        style["font_style"] = "italic"
    if value.get("fontStyle") == "bold":
        style["font_weight"] = 700
    return style


def generate(mode, data):
    colors = data["colors"]
    syntax = {capture: theme_style(data, scope) for capture, scope in ROLES.items()}
    for capture in ["punctuation.bracket", "punctuation.delimiter", "punctuation.special", "punctuation.list_marker"]:
        syntax[capture] = theme_style(data, "$foreground")
    # Each editor query uses these exact qualified names, including injections.
    for language, overrides in OVERRIDES.items():
        for capture, scope in overrides.items():
            syntax[f"{language}.{capture}"] = theme_style(data, scope)
    style = {
        "editor.background": colors["editor.background"], "editor.foreground": colors["editor.foreground"],
        "editor.gutter.background": colors["editor.background"],
        "editor.active_line.background": colors["editor.lineHighlightBackground"],
        "editor.line_number": colors["editorLineNumber.foreground"],
        "editor.active_line_number": colors["editorLineNumber.activeForeground"],
        "editor.invisible": colors["editorWhitespace.foreground"], "syntax": syntax,
    }
    for status, role in [("error", "variable"), ("warning", "type"), ("info", "function"), ("success", "string"), ("hint", "comment")]:
        color = syntax[role]["color"]
        style.update({status: color, status + ".border": color, status + ".background": color + "26"})
    return {
        "highlight_theme": {"name": f"Atom One {mode.title()}", "appearance": mode, "style": style},
        "selection": colors["editor.selectionBackground"], "caret": colors["editorCursor.foreground"],
        "search_match": colors["editor.findMatchHighlightBackground"], "search_match_active": colors["editor.selectionBackground"],
        "muted_foreground": colors["editorLineNumber.foreground"], "border": colors["editorIndentGuide.background"],
    }


def generate_github(data):
    tokens = data["tokens"]
    foreground = tokens["codeMirror-fgColor"]
    # GitHub's file-viewer syntax tokens, mapped to Tree-sitter roles.
    groups = {
        "keyword": "keyword conditional exception include repeat storageclass import cImport preproc tag.doctype operator",
        "entity": "function function.builtin function.method.builtin constructor method",
        "variable": "type enum variant variable variable.special variable.builtin variable.member property field",
        "constant": "constant number boolean float string.special.symbol",
        "string": "string string.special character text.literal text.code.span",
    }
    syntax = {}
    for token, captures in groups.items():
        for capture in captures.split():
            syntax[capture] = {"color": tokens[f"prettylights-syntax-{token}"]}
    for capture in ROLES:
        syntax.setdefault(capture, {"color": foreground})
    for capture in ["punctuation.bracket", "punctuation.delimiter", "punctuation.special", "punctuation.list_marker"]:
        syntax[capture] = {"color": foreground}
    for capture, token in {
        "type.builtin": "prettylights-syntax-constant",
        "string.escape": "prettylights-syntax-constant", "escape": "prettylights-syntax-constant",
        "attribute": "prettylights-syntax-constant", "label": "prettylights-syntax-constant",
        "comment": "prettylights-syntax-comment", "comment.doc": "prettylights-syntax-comment",
        "tag": "prettylights-syntax-entityTag", "string.regex": "prettylights-syntax-stringRegexp",
        "string.special.regex": "prettylights-syntax-stringRegexp", "link_text": "fgColor-accent",
        "link_uri": "prettylights-syntax-string", "text.uri": "prettylights-syntax-string",
        "text.reference": "fgColor-accent", "title": "prettylights-syntax-markup-heading",
    }.items():
        syntax[capture] = {"color": tokens[token]}
    syntax["emphasis"] = {"color": foreground, "font_style": "italic"}
    syntax["emphasis.strong"] = {"color": foreground, "font_weight": 700}
    syntax["embedded"] = {"color": foreground}
    # Share capture names with the dark palette, never its colors or TextMate rules.
    for language, overrides in OVERRIDES.items():
        for capture in overrides:
            base = capture
            while base not in syntax and "." in base:
                base = base.rsplit(".", 1)[0]
            syntax[f"{language}.{capture}"] = syntax.get(base, syntax["primary"]).copy()
    for language, captures in {
        "python": "variable constant property constructor variable.parameter type.parameter",
        "javascript": "variable variable.parameter punctuation.key_value variable.builtin",
        "typescript": "variable variable.parameter punctuation.key_value variable.builtin",
        "tsx": "variable variable.parameter punctuation.key_value variable.builtin",
        "c": "variable property", "cpp": "variable property",
        "java": "namespace variable.argument", "ruby": "variable",
        "rust": "type type.builtin variable.parameter",
        "sql": "variable field parameter type",
        "make": "string operator", "toml": "operator",
        "diff": "constant attribute keyword string variable.builtin",
    }.items():
        for capture in captures.split():
            syntax[f"{language}.{capture}"] = {"color": foreground}
    # New exact subroles fall back to their existing parent styles in One Dark.
    syntax["operator.reference"] = {"color": tokens["prettylights-syntax-constant"]}
    syntax["constructor.variant"] = {"color": tokens["prettylights-syntax-variable"]}
    syntax["variable.rust"] = {"color": foreground}
    syntax["property.rust"] = {"color": tokens["prettylights-syntax-constant"]}
    for language, capture, token in [
        ("python", "variable.builtin", "prettylights-syntax-constant"),
        ("python", "constant.builtin", "prettylights-syntax-constant"),
        ("javascript", "variable.constant", "prettylights-syntax-constant"),
        ("typescript", "variable.constant", "prettylights-syntax-constant"),
        ("tsx", "variable.constant", "prettylights-syntax-constant"),
        ("javascript", "string.special", "prettylights-syntax-stringRegexp"),
        ("typescript", "string.special", "prettylights-syntax-stringRegexp"),
        ("tsx", "string.special", "prettylights-syntax-stringRegexp"),
        ("rust", "attribute", "prettylights-syntax-constant"),
        ("css", "type", "prettylights-syntax-constant"),
        ("css", "property", "prettylights-syntax-constant"),
        ("css", "string.special", "prettylights-syntax-constant"),
        ("css", "attribute.id", "prettylights-syntax-entity"),
        ("css", "constant.color", "prettylights-syntax-constant"),
        ("css", "constant.builtin", "prettylights-syntax-constant"),
        ("c", "type.builtin", "prettylights-syntax-keyword"),
        ("cpp", "type.builtin", "prettylights-syntax-keyword"),
        ("java", "type.builtin", "prettylights-syntax-keyword"),
        ("go", "namespace", "prettylights-syntax-variable"),
        ("ruby", "constructor", "prettylights-syntax-variable"),
        ("make", "constant", "prettylights-syntax-variable"),
        ("toml", "type", "prettylights-syntax-constant"),
        ("toml", "property", "prettylights-syntax-constant"),
        ("markdown", "punctuation.heading", "prettylights-syntax-markup-heading"),
        ("markdown", "title.setext", "prettylights-syntax-markup-heading"),
        ("markdown_inline", "link_uri", "prettylights-syntax-string"),
        ("diff", "markup.inserted", "prettylights-syntax-markup-inserted-text"),
        ("diff", "markup.deleted", "prettylights-syntax-markup-deleted-text"),
    ]:
        syntax[f"{language}.{capture}"] = {"color": tokens[token]}
    style = {
        "editor.background": tokens["codeMirror-bgColor"], "editor.foreground": foreground,
        "editor.gutter.background": tokens["codeMirror-gutters-bgColor"],
        "editor.active_line.background": tokens["codeMirror-activeline-bgColor"],
        "editor.line_number": tokens["codeMirror-lineNumber-fgColor"],
        "editor.active_line_number": foreground,
        "editor.invisible": tokens["borderColor-default"], "syntax": syntax,
    }
    for status, token in [("error", "fgColor-danger"), ("warning", "fgColor-attention"),
                          ("info", "fgColor-accent"), ("success", "fgColor-success"), ("hint", "fgColor-muted")]:
        color = tokens[token]
        style.update({status: color, status + ".border": color, status + ".background": color + "26"})
    return {
        "highlight_theme": {"name": data["name"], "appearance": "light", "style": style},
        "selection": tokens["selection-bgColor"], "caret": tokens["codeMirror-cursor-fgColor"],
        "search_match": tokens["bgColor-attention-muted"], "search_match_active": tokens["codeMirror-selection-bgColor"],
        "muted_foreground": tokens["fgColor-muted"], "border": tokens["borderColor-default"],
    }


def load_source(source, mode):
    data = json.loads(source.read_text(encoding="utf-8"))
    fingerprint = hashlib.sha256(json.dumps(data, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    if fingerprint != FINGERPRINTS[mode]:
        raise ValueError(f"{source} does not match the pinned upstream theme")
    return data


def main():
    upstream = DEST / "upstream"
    upstream.mkdir(exist_ok=True)
    for mode, (repo, commit, filename) in SOURCES.items():
        source = upstream / f"{filename}.json"
        if not source.exists():
            source.write_bytes(urlopen(f"https://raw.githubusercontent.com/akamud/vscode-theme-{repo}/{commit}/themes/{filename}.json").read())
        data = load_source(source, mode)
        (DEST / f"atom-one-{mode}.json").write_bytes((json.dumps(generate(mode, data), indent=2) + "\n").encode("utf-8"))
        print(f"{mode}: retained {len(data['tokenColors'])} upstream rules; generated explicit scope mappings")
    light = generate_github(load_source(upstream / "GitHubLight.json", "light"))
    (DEST / "github-light.json").write_bytes((json.dumps(light, indent=2) + "\n").encode("utf-8"))
    print("light: generated GitHub file-viewer syntax and editor control mappings")


if __name__ == "__main__":
    main()
