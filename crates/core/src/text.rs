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

/// One rendered line of release notes, with just enough structure for a UI
/// to style it: headings get weight, bullets get a marker and indent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteLine {
    /// `#`-heading; level 1 is the largest.
    Heading {
        level: u8,
        text: String,
    },
    /// List item; `indent` counts nesting levels starting at 0.
    Bullet {
        indent: usize,
        text: String,
    },
    Text(String),
    Blank,
}

/// Parses Markdown into styleable lines: the structure that matters when
/// rendering (headings, bullets, paragraph breaks) survives, inline syntax
/// is flattened the same way as [`markdown_to_plain`].
pub fn markdown_note_lines(source: &str) -> Vec<NoteLine> {
    let mut out: Vec<NoteLine> = Vec::new();
    let mut in_code_block = false;

    for raw in source.lines() {
        let line = raw.trim_end();

        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            out.push(NoteLine::Text(line.to_string()));
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !matches!(out.last(), None | Some(NoteLine::Blank)) {
                out.push(NoteLine::Blank);
            }
            continue;
        }
        if trimmed.chars().all(|c| c == '-' || c == '*' || c == '_') && trimmed.len() >= 3 {
            continue;
        }

        let heading = trimmed.trim_start_matches('#');
        let hashes = trimmed.len() - heading.len();
        if hashes > 0 {
            out.push(NoteLine::Heading {
                level: hashes.min(6) as u8,
                text: strip_inline(heading.trim()),
            });
            continue;
        }

        let indent_chars = line.len() - line.trim_start().len();
        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
        {
            out.push(NoteLine::Bullet {
                indent: indent_chars / 2,
                text: strip_inline(rest.trim_start()),
            });
            continue;
        }

        out.push(NoteLine::Text(strip_inline(trimmed)));
    }

    while matches!(out.last(), Some(NoteLine::Blank)) {
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
    fn note_lines_keep_structure() {
        let notes =
            "# OptiScaler v0.9.4\n\nAdds **INT8** support.\n- New option\n  - nested detail";
        assert_eq!(
            markdown_note_lines(notes),
            vec![
                NoteLine::Heading {
                    level: 1,
                    text: "OptiScaler v0.9.4".into()
                },
                NoteLine::Blank,
                NoteLine::Text("Adds INT8 support.".into()),
                NoteLine::Bullet {
                    indent: 0,
                    text: "New option".into()
                },
                NoteLine::Bullet {
                    indent: 1,
                    text: "nested detail".into()
                },
            ]
        );
    }

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
