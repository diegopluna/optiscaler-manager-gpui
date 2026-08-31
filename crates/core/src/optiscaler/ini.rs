//! A line-preserving editor for `OptiScaler.ini`.
//!
//! OptiScaler ships its ini with ~1500 lines of comments documenting every
//! key's allowed values and default. Those comments are the only documentation
//! the user gets, so editing must never discard them: the document keeps every
//! line verbatim and only rewrites the value of a key that actually changed.
//!
//! The same comments are mined for a rough schema — whether a key is a
//! tri-state toggle, an enumeration, a number or free text — so the UI can show
//! a sensible control per key. Keys we cannot classify fall back to a text
//! field, which keeps the editor working against future OptiScaler versions.

use std::collections::BTreeMap;

/// What kind of control a key should get. Every OptiScaler key also accepts
/// `auto`, which is handled by the UI rather than encoded in each variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueType {
    /// `auto`, `true` or `false`.
    Bool,
    /// A fixed set of tokens, e.g. `fsr22`, `xess`, `dlss`.
    Enum(Vec<String>),
    Integer,
    Float,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    pub section: String,
    pub key: String,
    pub value: String,
    /// The comment block above the key, with the leading `;` stripped.
    pub help: Vec<String>,
    pub value_type: ValueType,
}

impl KeyInfo {
    /// Values to offer in a dropdown, `auto` first.
    pub fn choices(&self) -> Vec<String> {
        match &self.value_type {
            ValueType::Bool => vec!["auto".into(), "true".into(), "false".into()],
            ValueType::Enum(options) => {
                let mut choices = vec!["auto".to_string()];
                choices.extend(options.iter().cloned());
                choices
            }
            _ => Vec::new(),
        }
    }

    pub fn is_default(&self) -> bool {
        self.value.eq_ignore_ascii_case("auto")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LineKind {
    Blank,
    Comment,
    Section(String),
    Entry {
        key: String,
        value_span: (usize, usize),
    },
}

/// One source line, kept verbatim along with its original terminator so the
/// document can be written back byte for byte.
#[derive(Debug, Clone)]
struct Line {
    text: String,
    ending: &'static str,
    kind: LineKind,
}

/// A parsed `OptiScaler.ini` that can be edited and written back.
#[derive(Debug, Clone)]
pub struct IniDocument {
    lines: Vec<Line>,
}

impl IniDocument {
    pub fn parse(source: &str) -> Self {
        let mut lines = Vec::new();
        let mut rest = source;

        while !rest.is_empty() {
            let (raw, ending, remainder) = match rest.find('\n') {
                Some(ix) => {
                    let has_cr = ix > 0 && rest.as_bytes()[ix - 1] == b'\r';
                    let text_end = if has_cr { ix - 1 } else { ix };
                    (
                        &rest[..text_end],
                        if has_cr { "\r\n" } else { "\n" },
                        &rest[ix + 1..],
                    )
                }
                None => (rest, "", ""),
            };

            lines.push(Line {
                kind: classify(raw),
                text: raw.to_string(),
                ending,
            });
            rest = remainder;
        }

        IniDocument { lines }
    }

    /// Serializes the document. Unmodified documents round-trip byte for byte.
    pub fn to_source(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            out.push_str(&line.text);
            out.push_str(line.ending);
        }
        out
    }

    /// The current value of `key` in `section`.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        let mut current = String::new();
        for line in &self.lines {
            match &line.kind {
                LineKind::Section(name) => current = name.clone(),
                LineKind::Entry {
                    key: entry_key,
                    value_span,
                } if current.eq_ignore_ascii_case(section)
                    && entry_key.eq_ignore_ascii_case(key) =>
                {
                    return Some(&line.text[value_span.0..value_span.1]);
                }
                _ => {}
            }
        }
        None
    }

    /// Replaces the value of an existing key, leaving the key's spacing and
    /// any trailing comment untouched. Returns false if the key is not present.
    pub fn set(&mut self, section: &str, key: &str, value: &str) -> bool {
        let mut current = String::new();
        for line in &mut self.lines {
            match &line.kind {
                LineKind::Section(name) => current = name.clone(),
                LineKind::Entry {
                    key: entry_key,
                    value_span,
                } if current.eq_ignore_ascii_case(section)
                    && entry_key.eq_ignore_ascii_case(key) =>
                {
                    let (start, end) = *value_span;
                    let entry_key = entry_key.clone();
                    line.text.replace_range(start..end, value);
                    line.kind = LineKind::Entry {
                        key: entry_key,
                        value_span: (start, start + value.len()),
                    };
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Section names in file order.
    pub fn section_names(&self) -> Vec<String> {
        self.lines
            .iter()
            .filter_map(|line| match &line.kind {
                LineKind::Section(name) => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Every key, in file order, with the documentation and inferred type.
    pub fn keys(&self) -> Vec<KeyInfo> {
        let mut out = Vec::new();
        let mut section = String::new();
        let mut comments: Vec<String> = Vec::new();

        for line in &self.lines {
            match &line.kind {
                LineKind::Section(name) => {
                    section = name.clone();
                    comments.clear();
                }
                LineKind::Comment => {
                    let text = strip_comment(&line.text);
                    // Skip the `; ------` rules used to box section headings;
                    // they are decoration, not documentation.
                    if !is_separator(&text) {
                        comments.push(text);
                    }
                }
                LineKind::Blank => comments.clear(),
                // A run of consecutive keys shares the comment block above it,
                // as OptiScaler does for groups like the RenderPreset* keys,
                // so the block is kept until a blank line ends the group.
                LineKind::Entry { key, value_span } => {
                    let help = comments.clone();
                    out.push(KeyInfo {
                        section: section.clone(),
                        key: key.clone(),
                        value: line.text[value_span.0..value_span.1].to_string(),
                        value_type: infer_type(&help),
                        help,
                    });
                }
            }
        }
        out
    }

    /// Keys grouped by section, in file order.
    pub fn keys_by_section(&self) -> BTreeMap<String, Vec<KeyInfo>> {
        let mut map: BTreeMap<String, Vec<KeyInfo>> = BTreeMap::new();
        for info in self.keys() {
            map.entry(info.section.clone()).or_default().push(info);
        }
        map
    }
}

fn classify(raw: &str) -> LineKind {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return LineKind::Blank;
    }
    if trimmed.starts_with(';') || trimmed.starts_with('#') {
        return LineKind::Comment;
    }
    if trimmed.starts_with('[')
        && let Some(end) = trimmed.find(']')
    {
        return LineKind::Section(trimmed[1..end].trim().to_string());
    }

    // `key = value`, where the value runs to end of line. OptiScaler does not
    // use trailing comments on entries, so `;` inside a value is kept as-is.
    match raw.find('=') {
        Some(eq) => {
            let key = raw[..eq].trim();
            if key.is_empty() {
                return LineKind::Comment;
            }
            let after = &raw[eq + 1..];
            let leading = after.len() - after.trim_start().len();
            let start = eq + 1 + leading;
            let end = start + after.trim().len();
            LineKind::Entry {
                key: key.to_string(),
                value_span: (start, end),
            }
        }
        // Anything else is not something we understand; preserve it verbatim.
        None => LineKind::Comment,
    }
}

/// True for comment lines made only of rule characters.
fn is_separator(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| matches!(c, '-' | '=' | '*' | '_' | ' '))
}

fn strip_comment(raw: &str) -> String {
    raw.trim_start()
        .trim_start_matches([';', '#'])
        .trim()
        .to_string()
}

/// Infers a key's type from its documentation block.
fn infer_type(help: &[String]) -> ValueType {
    let joined = help.join(" ").to_lowercase();

    if joined.contains("true or false") || joined.contains("true/false") {
        return ValueType::Bool;
    }

    // Prefer the option list closest to the key.
    for line in help.iter().rev() {
        if let Some(options) = parse_options(line) {
            return ValueType::Enum(options);
        }
    }

    if joined.contains("float")
        || joined.contains("value range")
        || joined.contains(" to 2.0")
        || joined.contains("between")
    {
        return ValueType::Float;
    }
    if joined.contains("integer") {
        return ValueType::Integer;
    }

    ValueType::Text
}

/// Reads `a, b (note), c - Default (auto) is a` as the options `a`, `b`, `c`.
fn parse_options(line: &str) -> Option<Vec<String>> {
    // Drop the trailing "- Default ..." explanation if present.
    let head = match line.to_lowercase().find("- default") {
        Some(ix) => &line[..ix],
        None => line,
    };
    let head = head.trim();
    if head.is_empty() || head.ends_with('.') {
        return None;
    }

    let parts = split_top_level(head);
    if parts.len() < 2 {
        return None;
    }

    let mut options = Vec::new();
    for part in parts {
        // Each option is a bare token, optionally followed by a parenthesised
        // note: "ffx_12 (FSR 2.3; 3.1; 4.x)".
        let token = part.split_whitespace().next()?;
        if token.is_empty()
            || token.len() > 20
            || !token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return None;
        }
        options.push(token.to_string());
    }

    options.dedup();
    (options.len() >= 2).then_some(options)
}

/// Splits on commas that are not inside parentheses.
fn split_top_level(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (ix, ch) in input.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&input[start..ix]);
                start = ix + 1;
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
; -------------------------------------------------------
[Upscalers]
; -------------------------------------------------------
; Select Upscaler for Dx11 games
; fsr22 (native DX11), fsr31 (native DX11), xess (native DX11, Arc only), dlss - Default (auto) is fsr22
Dx11Upscaler=auto

[FrameGen]
; Enables Frame Generation
; true or false - Default (auto) is false
Enabled=auto

; Frame rate limit
; float - Default (auto) is 0.0 (disabled)
FramerateLimit=auto
";

    #[test]
    fn round_trips_unmodified_source_byte_for_byte() {
        let doc = IniDocument::parse(SAMPLE);
        assert_eq!(doc.to_source(), SAMPLE);
    }

    #[test]
    fn preserves_crlf_and_a_missing_final_newline() {
        let source = "[A]\r\nKey=auto\r\n; trailing comment";
        let doc = IniDocument::parse(source);
        assert_eq!(doc.to_source(), source);
    }

    #[test]
    fn setting_a_value_leaves_every_other_byte_alone() {
        let mut doc = IniDocument::parse(SAMPLE);
        assert!(doc.set("FrameGen", "Enabled", "true"));

        let updated = doc.to_source();
        assert_eq!(doc.get("FrameGen", "Enabled"), Some("true"));
        assert_eq!(updated, SAMPLE.replace("Enabled=auto", "Enabled=true"));
    }

    #[test]
    fn setting_the_same_key_twice_keeps_the_line_intact() {
        let mut doc = IniDocument::parse(SAMPLE);
        doc.set("FrameGen", "Enabled", "true");
        doc.set("FrameGen", "Enabled", "false");

        assert_eq!(doc.get("FrameGen", "Enabled"), Some("false"));
        assert_eq!(
            doc.to_source(),
            SAMPLE.replace("Enabled=auto", "Enabled=false")
        );
    }

    #[test]
    fn unknown_keys_are_not_invented() {
        let mut doc = IniDocument::parse(SAMPLE);
        assert!(!doc.set("FrameGen", "NoSuchKey", "true"));
        assert_eq!(doc.to_source(), SAMPLE);
    }

    #[test]
    fn infers_types_from_the_documentation_comments() {
        let doc = IniDocument::parse(SAMPLE);
        let keys = doc.keys();

        let upscaler = &keys[0];
        assert_eq!(upscaler.key, "Dx11Upscaler");
        assert_eq!(
            upscaler.value_type,
            ValueType::Enum(vec![
                "fsr22".into(),
                "fsr31".into(),
                "xess".into(),
                "dlss".into()
            ]),
            "commas inside parentheses must not split options"
        );
        assert_eq!(upscaler.choices()[0], "auto");

        assert_eq!(keys[1].value_type, ValueType::Bool);
        assert_eq!(keys[2].value_type, ValueType::Float);
    }

    #[test]
    fn keeps_the_comment_block_as_help_text() {
        let doc = IniDocument::parse(SAMPLE);
        let enabled = doc.keys().into_iter().find(|k| k.key == "Enabled").unwrap();

        assert_eq!(enabled.help[0], "Enables Frame Generation");
        assert!(enabled.help[1].starts_with("true or false"));
    }

    #[test]
    fn consecutive_keys_share_the_comment_block_above_them() {
        let source = "\
[DLSS]
; Render preset for each quality level
; Integer value - Default (auto) is 0
RenderPresetDLAA=auto
RenderPresetQuality=auto

UndocumentedKey=auto
";
        let keys = IniDocument::parse(source).keys();

        assert_eq!(keys[1].key, "RenderPresetQuality");
        assert_eq!(keys[1].help, keys[0].help, "inherits the group's help");
        assert_eq!(keys[1].value_type, ValueType::Integer);

        // A blank line ends the group.
        assert!(keys[2].help.is_empty());
        assert_eq!(keys[2].value_type, ValueType::Text);
    }

    #[test]
    fn prose_with_commas_is_not_mistaken_for_options() {
        assert_eq!(parse_options("VendorID to spoof, in hex."), None);
        assert_eq!(parse_options("0x10de = Nvidia | 0x8086 = Intel"), None);
    }
}
