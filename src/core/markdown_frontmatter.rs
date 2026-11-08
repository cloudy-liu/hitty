use std::borrow::Cow;

use serde_yaml::{Mapping, Value};

pub fn preprocess(content: &str) -> Cow<'_, str> {
    render(content)
        .map(Cow::Owned)
        .unwrap_or(Cow::Borrowed(content))
}

fn render(content: &str) -> Option<String> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = content.split_inclusive('\n');
    let opening = lines.next()?;
    if !opening.ends_with('\n') || opening.trim_end() != "---" {
        return None;
    }
    let start = opening.len();
    let mut end = start;
    for line in lines {
        if line.trim_end() == "---" {
            let mapping: Mapping = serde_yaml::from_str(&content[start..end]).ok()?;
            if mapping.is_empty() {
                return None;
            }
            let mut table = String::from("<table style=\"white-space: normal\">\n");
            for (key, value) in &mapping {
                table.push_str("<tr><th>");
                table.push_str(&cell(key)?);
                table.push_str("</th><td>");
                table.push_str(&cell(value)?);
                table.push_str("</td></tr>\n");
            }
            table.push_str("</table>\n\n");
            table.push_str(&content[end + line.len()..]);
            return Some(table);
        }
        end += line.len();
    }
    None
}

fn cell(value: &Value) -> Option<String> {
    let text = match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        value => serde_yaml::to_string(value).ok()?,
    };
    let text = text.trim_end_matches('\n');
    if text.is_empty() {
        return Some("&#32;".into());
    }
    Some(
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('\n', "<br>"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_rows_keep_source_order_and_the_document_body() {
        let content = "---\nname: show-me\ndescription: Show a diagram.\ndisable-model-invocation: true\n---\n\n# Body\n";
        for newline in ["\n", "\r\n"] {
            let content = content.replace('\n', newline);
            let processed = preprocess(&content);
            assert!(processed.starts_with(concat!(
                "<table style=\"white-space: normal\">\n",
                "<tr><th>name</th><td>show-me</td></tr>\n",
                "<tr><th>description</th><td>Show a diagram.</td></tr>\n",
                "<tr><th>disable-model-invocation</th><td>true</td></tr>\n",
                "</table>\n\n"
            )));
            assert!(processed.ends_with(&format!("{newline}# Body{newline}")));
        }
    }

    #[test]
    fn metadata_accepts_a_bom_trailing_spaces_and_an_eof_delimiter() {
        let content = "\u{feff}--- \r\nname: show-me\r\n---\t";
        assert!(preprocess(content).starts_with("<table style=\"white-space: normal\">\n"));
        assert!(preprocess(content).ends_with("</table>\n\n"));
    }

    #[test]
    fn metadata_escapes_markup_and_preserves_literal_values() {
        let content = "---\n'<b>key</b>': '<img src=\"https://example.invalid/image\"> & *literal*'\nempty: ''\nmissing: null\n---\n";
        let processed = preprocess(content);
        assert!(processed.contains("&lt;b&gt;key&lt;/b&gt;"));
        assert!(
            processed.contains("&lt;img src=\"https://example.invalid/image\"&gt; &amp; *literal*")
        );
        assert!(!processed.contains("<img"));
        assert!(processed.contains("<th>empty</th><td>&#32;</td>"));
        assert!(processed.contains("<th>missing</th><td>&#32;</td>"));
    }

    #[test]
    fn metadata_folds_prose_and_keeps_multiline_and_nested_values_readable() {
        let content = "---\ndescription: >-\n  Show a diagram\n  for the user.\nnotes: |-\n  first line\n  second line\ntags: [one, two]\nmetadata:\n  author: example\n---\n";
        let processed = preprocess(content);
        assert!(processed.contains("<td>Show a diagram for the user.</td>"));
        assert!(processed.contains("<td>first line<br>second line</td>"));
        assert!(processed.contains("<td>- one<br>- two</td>"));
        assert!(processed.contains("<td>author: example</td>"));
    }

    #[test]
    fn invalid_or_nonleading_metadata_stays_unchanged() {
        for content in [
            "# Body\n\n---\nname: show-me\n---\n",
            "\n---\nname: show-me\n---\n",
            "---\nname: show-me\n",
            "---\nnot a mapping\n---\n",
            "---\n- one\n- two\n---\n",
            "---\nname: [broken\n---\n",
            "---\nname: first\nname: second\n---\n",
            "---\n---\nBody\n",
            "```yaml\n---\nname: show-me\n---\n```\n",
            "---\n\nOrdinary text\n",
        ] {
            assert!(matches!(preprocess(content), Cow::Borrowed(original) if original == content));
        }
    }
}
