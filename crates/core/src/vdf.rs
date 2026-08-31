//! Minimal parser for Valve's KeyValues text format, used by
//! `libraryfolders.vdf` and `appmanifest_*.acf`.
//!
//! The format is a tree of quoted strings: a key is either followed by a
//! quoted value or by a `{ ... }` block. Line comments start with `//`.
//! Conditional suffixes (`[$WIN32]`) are ignored, as are unquoted tokens,
//! neither of which appear in the files we read.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    String(String),
    Block(BTreeMap<String, Value>),
}

impl Value {
    /// The string at `path`, e.g. `get_str(["AppState", "name"])`.
    pub fn get_str<'a>(&self, path: impl IntoIterator<Item = &'a str>) -> Option<&str> {
        match self.get(path) {
            Some(Value::String(s)) => Some(s),
            _ => None,
        }
    }

    /// The node at `path`, walking nested blocks. Key lookup is
    /// case-insensitive, since Valve is inconsistent about casing.
    pub fn get<'a>(&self, path: impl IntoIterator<Item = &'a str>) -> Option<&Value> {
        let mut node = self;
        for key in path {
            let Value::Block(map) = node else {
                return None;
            };
            node = map.get(key).or_else(|| {
                map.iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .map(|(_, v)| v)
            })?;
        }
        Some(node)
    }

    pub fn as_block(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Block(map) => Some(map),
            Value::String(_) => None,
        }
    }
}

/// Parses a KeyValues document into its (single) root block.
///
/// Malformed input yields whatever was parsed up to that point rather than an
/// error: these files are written by Steam and a partial read is more useful
/// than dropping a whole library.
pub fn parse(input: &str) -> BTreeMap<String, Value> {
    let mut chars = input.char_indices().peekable();
    let mut root = BTreeMap::new();
    parse_block_body(input, &mut chars, &mut root, 0);
    root
}

type Cursor<'a> = std::iter::Peekable<std::str::CharIndices<'a>>;

fn parse_block_body(
    input: &str,
    chars: &mut Cursor<'_>,
    out: &mut BTreeMap<String, Value>,
    depth: usize,
) {
    // Guard against pathological nesting in a corrupt file.
    if depth > 64 {
        return;
    }

    loop {
        skip_trivia(chars);
        match chars.peek().map(|(_, c)| *c) {
            None => return,
            Some('}') => {
                chars.next();
                return;
            }
            Some('"') => {}
            Some(_) => {
                // Unexpected token; skip it so one bad line can't stall us.
                chars.next();
                continue;
            }
        }

        let Some(key) = read_quoted(input, chars) else {
            return;
        };

        skip_trivia(chars);
        match chars.peek().map(|(_, c)| *c) {
            Some('"') => {
                if let Some(value) = read_quoted(input, chars) {
                    out.insert(key, Value::String(value));
                }
            }
            Some('{') => {
                chars.next();
                let mut nested = BTreeMap::new();
                parse_block_body(input, chars, &mut nested, depth + 1);
                out.insert(key, Value::Block(nested));
            }
            _ => return,
        }
    }
}

fn skip_trivia(chars: &mut Cursor<'_>) {
    loop {
        match chars.peek().map(|(_, c)| *c) {
            Some(c) if c.is_whitespace() => {
                chars.next();
            }
            Some('/') => {
                // Line comment; consume to end of line.
                chars.next();
                if chars.peek().map(|(_, c)| *c) == Some('/') {
                    for (_, c) in chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                } else {
                    return;
                }
            }
            _ => return,
        }
    }
}

/// Reads a `"..."` token, resolving the escapes Steam actually emits.
fn read_quoted(_input: &str, chars: &mut Cursor<'_>) -> Option<String> {
    if chars.next().map(|(_, c)| c) != Some('"') {
        return None;
    }

    let mut out = String::new();
    while let Some((_, c)) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next().map(|(_, c)| c) {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => out.push(other),
                None => return Some(out),
            },
            _ => out.push(c),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_blocks_and_escapes() {
        let src = r#"
            // a comment
            "libraryfolders"
            {
                "0"
                {
                    "path"    "C:\\Program Files (x86)\\Steam"
                    "apps"
                    {
                        "570"    "1234"
                    }
                }
            }
        "#;

        let root = Value::Block(parse(src));
        assert_eq!(
            root.get_str(["libraryfolders", "0", "path"]),
            Some(r"C:\Program Files (x86)\Steam")
        );
        assert_eq!(
            root.get_str(["libraryfolders", "0", "apps", "570"]),
            Some("1234")
        );
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let root = Value::Block(parse(r#""AppState" { "Name" "Dota 2" }"#));
        assert_eq!(root.get_str(["appstate", "name"]), Some("Dota 2"));
    }

    #[test]
    fn truncated_input_yields_what_was_parsed() {
        let root = Value::Block(parse(r#""AppState" { "name" "Half-Life"#));
        // The unterminated value is still recovered rather than discarded.
        assert_eq!(root.get_str(["AppState", "name"]), Some("Half-Life"));
    }
}
