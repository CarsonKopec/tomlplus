//! Logical-line lexer.
//!
//! Splits source into [`LineToken`]s, joining continuation lines whose
//! brackets are still open (so multi-line arrays / inline dicts arrive
//! at the parser as a single `KV` token).
//!
//! Block dicts (`key = #{` opening on its own line, terminated by a `}#`
//! line) stay multi-token so the parser can attach per-key annotations.

use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Blank,
    Section,
    Vars,
    Annotation,
    Kv,
    BlockOpen,
    BlockClose,
}

#[derive(Debug, Clone)]
pub struct LineToken {
    pub kind: LineKind,
    /// Original source slice (joined for multi-line KV values).
    pub raw: String,
    /// Source span covering `raw`.
    pub span: Span,
    /// Stripped/comment-removed content used by the parser.
    pub content: String,
    /// 1-indexed line number of the first physical line of this token.
    pub lineno: u32,
    /// For `Kv` / `BlockOpen` — unquoted key name.
    pub key: String,
    /// Span of the key text in the source.
    pub key_span: Span,
    /// For `Kv` — the right-hand side (post-comment-stripping, joined).
    pub value: String,
    /// Span of the value text in the source.
    pub value_span: Span,
    /// For `Section` — raw section name (still dot-encoded).
    pub section: String,
    /// Span of the section name.
    pub section_span: Span,
    /// For `Annotation` — full `@name…` text.
    pub annotation_text: String,
}

impl LineToken {
    fn empty(kind: LineKind, raw: String, content: String, lineno: u32, span: Span) -> Self {
        Self {
            kind,
            raw,
            span,
            content,
            lineno,
            key: String::new(),
            key_span: Span::DUMMY,
            value: String::new(),
            value_span: Span::DUMMY,
            section: String::new(),
            section_span: Span::DUMMY,
            annotation_text: String::new(),
        }
    }
}

/// Split a section name on `.` outside of quoted strings.
/// `a.b.c` -> ["a","b","c"]; `"a.b".c` -> ["a.b","c"].
pub fn split_section_path(name: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut buf = String::new();
    let mut in_str = false;
    let mut prev = '\0';
    for ch in name.chars() {
        if ch == '"' && prev != '\\' {
            in_str = !in_str;
            buf.push(ch);
        } else if ch == '.' && !in_str {
            parts.push(unquote_key(buf.trim()));
            buf = String::new();
        } else {
            buf.push(ch);
        }
        prev = ch;
    }
    parts.push(unquote_key(buf.trim()));
    parts
}

pub fn unquote_key(k: &str) -> String {
    if k.len() >= 2 && k.starts_with('"') && k.ends_with('"') {
        let inner = &k[1..k.len() - 1];
        inner.replace("\\\"", "\"").replace("\\\\", "\\")
    } else {
        k.to_string()
    }
}

/// Strip a trailing `# comment`, respecting strings and `#{` / `}#` pairs.
/// Returns the comment-stripped portion of the line (no trailing whitespace).
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut esc = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if esc {
            esc = false;
            i += 1;
            continue;
        }
        if b == b'\\' {
            esc = true;
            i += 1;
            continue;
        }
        if b == b'"' {
            in_str = !in_str;
            i += 1;
            continue;
        }
        if b == b'#' && !in_str {
            // `#{` opens a dict — not a comment
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                i += 2;
                continue;
            }
            // `}#` closes a dict — the `#` here is part of the delimiter
            let preceding = line[..i].trim_end();
            if preceding.ends_with('}') {
                i += 1;
                continue;
            }
            return line[..i].trim_end();
        }
        i += 1;
    }
    line.trim_end()
}

/// Net bracket depth contributed by a slice of content (after comment strip).
fn bracket_delta(content: &str) -> i32 {
    let bytes = content.as_bytes();
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut esc = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if esc {
            esc = false;
            i += 1;
            continue;
        }
        if b == b'\\' {
            esc = true;
            i += 1;
            continue;
        }
        if b == b'"' {
            in_str = !in_str;
            i += 1;
            continue;
        }
        if !in_str {
            if b == b'#' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                depth += 1;
                i += 2;
                continue;
            }
            if b == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'#' {
                depth -= 1;
                i += 2;
                continue;
            }
            match b {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    depth
}

/// Iterate physical lines as `(line_number_1based, byte_offset_of_start, slice)`.
fn physical_lines(source: &str) -> Vec<(u32, usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut lineno: u32 = 1;
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            // Strip a trailing \r if present (CRLF tolerance)
            let end = if i > start && bytes[i - 1] == b'\r' { i - 1 } else { i };
            out.push((lineno, start, &source[start..end]));
            start = i + 1;
            lineno += 1;
        }
        i += 1;
    }
    if start < bytes.len() {
        out.push((lineno, start, &source[start..]));
    } else if source.is_empty() {
        // No-op; produces no lines, matching Python `splitlines`.
    }
    out
}

pub fn tokenize(source: &str) -> Vec<LineToken> {
    let lines = physical_lines(source);
    let mut tokens: Vec<LineToken> = Vec::with_capacity(lines.len());

    let mut i = 0usize;
    while i < lines.len() {
        let (lineno, line_start, raw_line) = lines[i];
        let line_end = line_start + raw_line.len();
        let span = Span::new(line_start, line_end);

        let stripped = strip_comment(raw_line);
        let content = stripped.trim();

        if content.is_empty() {
            tokens.push(LineToken::empty(
                LineKind::Blank,
                raw_line.to_string(),
                String::new(),
                lineno,
                span,
            ));
            i += 1;
            continue;
        }

        if content == "[vars]" {
            tokens.push(LineToken::empty(
                LineKind::Vars,
                raw_line.to_string(),
                content.to_string(),
                lineno,
                span,
            ));
            i += 1;
            continue;
        }

        // Section header: `[ ... ]` (allow whitespace).
        if let Some(name) = match_section(content) {
            // Map section text span to source span — content starts at line_start
            // plus an offset (how much leading whitespace).
            let lead = raw_line.len() - raw_line.trim_start().len();
            let section_offset_in_line = lead + 1; // skip `[`
            let section_start = line_start + section_offset_in_line;
            let section_span = Span::new(section_start, section_start + name.len());
            let mut tok = LineToken::empty(
                LineKind::Section,
                raw_line.to_string(),
                content.to_string(),
                lineno,
                span,
            );
            tok.section = name.to_string();
            tok.section_span = section_span;
            tokens.push(tok);
            i += 1;
            continue;
        }

        // Annotation
        if content.starts_with('@') {
            let mut tok = LineToken::empty(
                LineKind::Annotation,
                raw_line.to_string(),
                content.to_string(),
                lineno,
                span,
            );
            tok.annotation_text = content.to_string();
            tokens.push(tok);
            i += 1;
            continue;
        }

        // Block dict close
        if content == "}#" {
            tokens.push(LineToken::empty(
                LineKind::BlockClose,
                raw_line.to_string(),
                content.to_string(),
                lineno,
                span,
            ));
            i += 1;
            continue;
        }

        // KV
        if let Some((key, key_offset, value, value_offset)) = match_kv(raw_line, stripped) {
            let key_span = Span::new(line_start + key_offset, line_start + key_offset + key.len());
            let mut value_span = Span::new(
                line_start + value_offset,
                line_start + value_offset + value.len(),
            );

            // Block-open: `key = #{` exactly.
            let trimmed_value = value.trim();
            if trimmed_value == "#{" {
                let mut tok = LineToken::empty(
                    LineKind::BlockOpen,
                    raw_line.to_string(),
                    stripped.trim_start().to_string(),
                    lineno,
                    span,
                );
                tok.key = unquote_key(&key);
                tok.key_span = key_span;
                tokens.push(tok);
                i += 1;
                continue;
            }

            // Multi-line value continuation
            let mut joined_raw = raw_line.to_string();
            let mut joined_value = value.clone();
            let mut depth = bracket_delta(&joined_value);
            let mut last_end = line_end;
            while depth > 0 && i + 1 < lines.len() {
                i += 1;
                let (_next_lineno, next_start, next_raw) = lines[i];
                let next_stripped = strip_comment(next_raw).trim();
                joined_raw.push('\n');
                joined_raw.push_str(next_raw);
                joined_value.push(' ');
                joined_value.push_str(next_stripped);
                depth += bracket_delta(next_stripped);
                last_end = next_start + next_raw.len();
            }
            value_span.end = last_end;
            let total_span = Span::new(line_start, last_end);

            let mut tok = LineToken::empty(
                LineKind::Kv,
                joined_raw,
                stripped.trim_start().to_string(),
                lineno,
                total_span,
            );
            tok.key = unquote_key(&key);
            tok.key_span = key_span;
            tok.value = joined_value;
            tok.value_span = value_span;
            tokens.push(tok);
            i += 1;
            continue;
        }

        // Fall-through: blank-equivalent
        tokens.push(LineToken::empty(
            LineKind::Blank,
            raw_line.to_string(),
            content.to_string(),
            lineno,
            span,
        ));
        i += 1;
    }

    tokens
}

/// Match a line as `[ name ]` and return the trimmed `name`.
fn match_section(content: &str) -> Option<&str> {
    let bytes = content.as_bytes();
    if bytes.first() != Some(&b'[') || bytes.last() != Some(&b']') {
        return None;
    }
    let inner = content[1..content.len() - 1].trim();
    if inner.is_empty() || inner.contains('[') || inner.contains(']') {
        return None;
    }
    Some(inner)
}

/// Match a `key = value` line and return `(key, key_offset, value, value_offset)`.
/// Offsets are relative to the start of the *raw* line (so they map to source
/// byte offsets when added to the line's start offset).
fn match_kv(raw_line: &str, stripped: &str) -> Option<(String, usize, String, usize)> {
    let leading = raw_line.len() - raw_line.trim_start().len();
    let trimmed = &stripped[leading.min(stripped.len())..stripped.len().min(stripped.len())];
    // Use the trimmed-content variant for parsing: `key = value` after
    // leading-space trimming. Operate on `stripped` shifted by `leading`.
    let work = &stripped[leading.min(stripped.len())..];
    if work.is_empty() {
        return None;
    }

    // Parse key
    let (key, key_len) = if work.starts_with('"') {
        // Quoted key
        let bytes = work.as_bytes();
        let mut j = 1;
        let mut esc = false;
        while j < bytes.len() {
            if esc {
                esc = false;
                j += 1;
                continue;
            }
            if bytes[j] == b'\\' {
                esc = true;
                j += 1;
                continue;
            }
            if bytes[j] == b'"' {
                j += 1;
                break;
            }
            j += 1;
        }
        if j > bytes.len() || work.as_bytes().get(j - 1) != Some(&b'"') {
            return None;
        }
        let k = &work[..j];
        (k.to_string(), j)
    } else {
        // Bare key: [A-Za-z0-9_-]+
        let bytes = work.as_bytes();
        let mut j = 0;
        while j < bytes.len() {
            let b = bytes[j];
            let ok = b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
            if !ok {
                break;
            }
            j += 1;
        }
        if j == 0 {
            return None;
        }
        (work[..j].to_string(), j)
    };

    // Skip optional whitespace, then `=`
    let after_key = &work[key_len..];
    let after_key_trim = after_key.trim_start();
    let ws1 = after_key.len() - after_key_trim.len();
    if !after_key_trim.starts_with('=') {
        return None;
    }
    let after_eq = &after_key_trim[1..];
    let after_eq_trim = after_eq.trim_start();
    let ws2 = after_eq.len() - after_eq_trim.len();
    if after_eq_trim.is_empty() {
        return None;
    }

    let key_offset = leading;
    let value_offset = leading + key_len + ws1 + 1 + ws2;
    let value = after_eq_trim.to_string();
    let _ = trimmed; // unused but kept for future col-mapping
    Some((key, key_offset, value, value_offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(toks: &[LineToken]) -> Vec<LineKind> {
        toks.iter().map(|t| t.kind).collect()
    }

    #[test]
    fn simple_kv() {
        let toks = tokenize("port = 8080");
        assert_eq!(kinds(&toks), vec![LineKind::Kv]);
        assert_eq!(toks[0].key, "port");
        assert_eq!(toks[0].value, "8080");
    }

    #[test]
    fn section() {
        let toks = tokenize("[server]\nport = 8080");
        assert_eq!(kinds(&toks), vec![LineKind::Section, LineKind::Kv]);
        assert_eq!(toks[0].section, "server");
    }

    #[test]
    fn dotted_section() {
        let toks = tokenize("[a.b.c]");
        assert_eq!(toks[0].section, "a.b.c");
        assert_eq!(split_section_path(&toks[0].section), vec!["a", "b", "c"]);
    }

    #[test]
    fn quoted_key() {
        let toks = tokenize("\"my key\" = 1");
        assert_eq!(toks[0].kind, LineKind::Kv);
        assert_eq!(toks[0].key, "my key");
    }

    #[test]
    fn annotation_then_kv() {
        let toks = tokenize("@type: int\nport = 80");
        assert_eq!(kinds(&toks), vec![LineKind::Annotation, LineKind::Kv]);
        assert_eq!(toks[0].annotation_text, "@type: int");
    }

    #[test]
    fn block_open_close() {
        let toks = tokenize("headers = #{\n  ct = \"json\"\n}#");
        assert_eq!(
            kinds(&toks),
            vec![LineKind::BlockOpen, LineKind::Kv, LineKind::BlockClose]
        );
    }

    #[test]
    fn multiline_array_joined() {
        let toks = tokenize("tags = [\n  \"a\",\n  \"b\",\n]");
        assert_eq!(kinds(&toks), vec![LineKind::Kv]);
        assert!(toks[0].value.contains('"'));
        assert!(toks[0].value.contains(']'));
    }

    #[test]
    fn comment_after_block_close() {
        let toks = tokenize("opts = #{ a = 1 }# # tail");
        assert_eq!(toks[0].kind, LineKind::Kv);
        assert!(!toks[0].content.contains("tail"));
    }

    #[test]
    fn split_path_quoted() {
        assert_eq!(split_section_path("\"a.b\".c"), vec!["a.b", "c"]);
    }
}
