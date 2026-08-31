//! Turns the Markdown GitHub stores release notes in into plain lines the UI
//! can render, since the app has no Markdown renderer and raw `##` and `**`
//! markers make a changelog harder to read rather than easier.

/// Flattens Markdown to plain text lines, keeping the structure that carries
/// meaning (headings, bullets, blank lines between paragraphs) and dropping
/// the syntax that does not render.
pub fn markdown_to_plain(source: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_code_block = false;

    for raw in source.lines() {
        let line = raw.trim_end();

        // Fenced code blocks are passed through verbatim, minus the fences.
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            out.push(line.to_string());
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            // Collapse runs of blank lines into a single separator, and drop
            // leading ones entirely.
            let trailing_blank = out.last().is_none_or(String::is_empty);
            if !trailing_blank {
                out.push(String::new());
            }
            continue;
        }

        // A horizontal rule is a separator, not content.
        if trimmed.chars().all(|c| c == '-' || c == '*' || c == '_') && trimmed.len() >= 3 {
            continue;
        }

        let heading = trimmed.trim_start_matches('#');
        let is_heading = heading.len() != trimmed.len();
        let mut text = if is_heading {
            heading.trim().to_string()
        } else {
            trimmed.to_string()
        };

        // Bullets: normalise the marker so nested lists still read as lists.
        let indent = line.len() - line.trim_start().len();
        if let Some(rest) = text
            .strip_prefix("- ")
            .or_else(|| text.strip_prefix("* "))
            .or_else(|| text.strip_prefix("+ "))
        {
            text = format!("{}• {}", " ".repeat(indent), rest.trim_start());
        }

        out.push(strip_inline(&text));
    }

    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

/// Removes inline emphasis, code ticks and link syntax, keeping link text.
fn strip_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut ix = 0;

    while ix < chars.len() {
        match chars[ix] {
            // `**bold**`, `__bold__`, `*italic*`, `_italic_`, `` `code` ``
            '*' | '_' | '`' => {
                let marker = chars[ix];
                let mut run = 0;
                while ix + run < chars.len() && chars[ix + run] == marker {
                    run += 1;
                }
                ix += run;
            }
            // `[text](url)` keeps only the text.
            '[' => {
                let close = (ix + 1..chars.len()).find(|&i| chars[i] == ']');
                match close {
                    Some(close) if chars.get(close + 1) == Some(&'(') => {
                        let paren = (close + 2..chars.len()).find(|&i| chars[i] == ')');
                        out.extend(&chars[ix + 1..close]);
                        ix = paren.map(|p| p + 1).unwrap_or(close + 1);
                    }
                    _ => {
                        out.push('[');
                        ix += 1;
                    }
                }
            }
            other => {
                out.push(other);
                ix += 1;
            }
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_headings_and_emphasis() {
        let notes =
            "# OptiScaler v0.9.4\n\n## Highlights\nAdds **proper support** for `FSR 4.1.1`.";
        assert_eq!(
            markdown_to_plain(notes),
            vec![
                "OptiScaler v0.9.4".to_string(),
                String::new(),
                "Highlights".to_string(),
                "Adds proper support for FSR 4.1.1.".to_string(),
            ]
        );
    }

    #[test]
    fn normalises_bullets_and_keeps_nesting() {
        let notes = "- First item\n* Second item\n  - Nested item";
        assert_eq!(
            markdown_to_plain(notes),
            vec![
                "• First item".to_string(),
                "• Second item".to_string(),
                "  • Nested item".to_string(),
            ]
        );
    }

    #[test]
    fn keeps_link_text_and_drops_the_url() {
        let notes = "See [the wiki](https://example.test/wiki) for details.";
        assert_eq!(
            markdown_to_plain(notes),
            vec!["See the wiki for details.".to_string()]
        );
    }

    #[test]
    fn collapses_blank_runs_and_drops_rules() {
        let notes = "One\n\n\n\n---\n\nTwo\n\n";
        assert_eq!(
            markdown_to_plain(notes),
            vec!["One".to_string(), String::new(), "Two".to_string()]
        );
    }

    #[test]
    fn passes_code_blocks_through_without_fences() {
        let notes = "Run:\n```\nDx12Upscaler=xess\n```";
        assert_eq!(
            markdown_to_plain(notes),
            vec!["Run:".to_string(), "Dx12Upscaler=xess".to_string()]
        );
    }
}
