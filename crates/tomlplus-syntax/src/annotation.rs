//! `@annotation` line parsing.
//!
//! Surface forms:
//!     `@name`                — flag
//!     `@name: arg`           — colon-form (most validators)
//!     `@name(arg)`           — parenthesised form (`@deprecated("msg")`)
//!
//! Captures sub-spans for both the `@name` head and the argument so the LSP
//! can paint them with different semantic-token types.

use crate::error::ParseError;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationArg {
    None,
    String(String),
    Int(i64),
    Float(f64),
    List(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub name: String,
    pub arg: AnnotationArg,
    /// Span of the whole annotation line, including `@`.
    pub span: Span,
    /// Span of `@name` head (decorator-style highlighting target).
    pub name_span: Span,
    /// Span of the argument text (after `:` or inside `(...)`). `None` for flags.
    pub arg_span: Option<Span>,
    /// For `@enum: [a, b, c]` — span of each list element.
    pub list_item_spans: Vec<Span>,
}

impl Annotation {
    pub fn is_metadata(&self) -> bool {
        matches!(
            self.name.as_str(),
            "required" | "deprecated" | "internal" | "readonly" | "experimental"
        )
    }

    pub fn is_validation(&self) -> bool {
        matches!(
            self.name.as_str(),
            "min"
                | "max"
                | "minlen"
                | "maxlen"
                | "pattern"
                | "enum"
                | "nonzero"
                | "positive"
                | "nonempty"
        )
    }

    pub fn is_type(&self) -> bool {
        self.name == "type"
    }

    pub fn is_tag(&self) -> bool {
        self.name == "tag"
    }
}

/// Parse a single annotation line.  `line_span` is the source-relative span of
/// the entire line content (starting at `@`).
pub fn parse_annotation(line: &str, line_span: Span) -> Result<Annotation, ParseError> {
    let trimmed = line.trim();
    if !trimmed.starts_with('@') {
        return Err(ParseError::Generic {
            message: "annotation must start with `@`".into(),
            span: line_span,
        });
    }

    // Read identifier after `@`.
    let after_at = &trimmed[1..];
    let bytes = after_at.as_bytes();
    let mut name_len = 0usize;
    while name_len < bytes.len() {
        let b = bytes[name_len];
        let ok = (name_len == 0 && (b.is_ascii_alphabetic() || b == b'_'))
            || (name_len > 0 && (b.is_ascii_alphanumeric() || b == b'_'));
        if !ok {
            break;
        }
        name_len += 1;
    }
    if name_len == 0 {
        return Err(ParseError::Generic {
            message: "annotation missing name".into(),
            span: line_span,
        });
    }
    let name = after_at[..name_len].to_string();
    // `@name` (incl. `@`) occupies bytes [0..name_len+1] of `trimmed`.
    let head_len = name_len + 1;
    let name_span = Span::new(line_span.start, line_span.start + head_len);

    let rest = after_at[name_len..].trim_start();
    let rest_offset_in_trimmed = (trimmed.len() - rest.len()).saturating_sub(0);

    let (arg, arg_span, list_item_spans) = if rest.is_empty() {
        (AnnotationArg::None, None, Vec::new())
    } else if let Some(after_colon) = rest.strip_prefix(':') {
        let arg_str = after_colon.trim();
        let leading = after_colon.len() - after_colon.trim_start().len();
        let trailing = after_colon.len() - after_colon.trim_end().len();
        // `+ 1` skips the `:` itself.
        let arg_start = line_span.start + rest_offset_in_trimmed + 1 + leading;
        let arg_end = line_span.start + rest_offset_in_trimmed + 1 + after_colon.len() - trailing;
        let span = Span::new(arg_start, arg_end);
        let (parsed, item_spans) = parse_arg(arg_str, span)?;
        (parsed, Some(span), item_spans)
    } else if rest.starts_with('(') && rest.ends_with(')') {
        let inner_raw = &rest[1..rest.len() - 1];
        let inner = inner_raw.trim();
        let leading = inner_raw.len() - inner_raw.trim_start().len();
        let trailing = inner_raw.len() - inner_raw.trim_end().len();
        let inner_start = line_span.start + rest_offset_in_trimmed + 1 + leading;
        let inner_end = line_span.start + rest_offset_in_trimmed + 1 + inner_raw.len() - trailing;
        let span = Span::new(inner_start, inner_end);
        let stripped = strip_str_quotes(inner);
        (
            AnnotationArg::String(stripped.to_string()),
            Some(span),
            Vec::new(),
        )
    } else {
        return Err(ParseError::Generic {
            message: format!("invalid annotation syntax: `{}`", line),
            span: line_span,
        });
    };

    Ok(Annotation {
        name,
        arg,
        span: line_span,
        name_span,
        arg_span,
        list_item_spans,
    })
}

fn parse_arg(raw: &str, arg_span: Span) -> Result<(AnnotationArg, Vec<Span>), ParseError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(ParseError::Generic {
            message: "annotation argument is empty".into(),
            span: arg_span,
        });
    }
    if raw.starts_with('[') && raw.ends_with(']') {
        let inner = &raw[1..raw.len() - 1];
        let inner_start = arg_span.start + 1;
        let mut parts = Vec::new();
        let mut spans = Vec::new();
        let mut cursor = 0usize;
        for chunk in inner.split(',') {
            let leading = chunk.len() - chunk.trim_start().len();
            let trailing = chunk.len() - chunk.trim_end().len();
            let trimmed = chunk.trim();
            if !trimmed.is_empty() {
                let unquoted = strip_str_quotes(trimmed);
                parts.push(unquoted.to_string());
                let item_start = inner_start + cursor + leading;
                let item_end = inner_start + cursor + chunk.len() - trailing;
                spans.push(Span::new(item_start, item_end));
            }
            cursor += chunk.len() + 1; // include the `,`
        }
        return Ok((AnnotationArg::List(parts), spans));
    }
    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        return Ok((
            AnnotationArg::String(raw[1..raw.len() - 1].to_string()),
            Vec::new(),
        ));
    }
    if let Ok(n) = raw.parse::<i64>() {
        return Ok((AnnotationArg::Int(n), Vec::new()));
    }
    if let Ok(f) = raw.parse::<f64>() {
        return Ok((AnnotationArg::Float(f), Vec::new()));
    }
    Ok((AnnotationArg::String(raw.to_string()), Vec::new()))
}

fn strip_str_quotes(s: &str) -> &str {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Annotation {
        parse_annotation(s, Span::new(0, s.len())).unwrap()
    }

    #[test]
    fn flag() {
        let a = parse("@required");
        assert_eq!(a.name, "required");
        assert_eq!(a.arg, AnnotationArg::None);
        assert_eq!(a.name_span, Span::new(0, 9));
        assert!(a.arg_span.is_none());
    }

    #[test]
    fn colon_int_has_arg_span() {
        let a = parse("@min: 1");
        assert_eq!(a.arg, AnnotationArg::Int(1));
        let arg_span = a.arg_span.unwrap();
        assert_eq!(&"@min: 1"[arg_span.start..arg_span.end], "1");
    }

    #[test]
    fn colon_string_arg_span() {
        let s = "@type: int";
        let a = parse(s);
        let arg_span = a.arg_span.unwrap();
        assert_eq!(&s[arg_span.start..arg_span.end], "int");
    }

    #[test]
    fn colon_list_item_spans() {
        let s = "@enum: [debug, info, warn]";
        let a = parse(s);
        assert_eq!(a.list_item_spans.len(), 3);
        let items: Vec<&str> = a
            .list_item_spans
            .iter()
            .map(|sp| &s[sp.start..sp.end])
            .collect();
        assert_eq!(items, vec!["debug", "info", "warn"]);
    }

    #[test]
    fn deprecated_paren_arg_span() {
        let s = "@deprecated(\"use new\")";
        let a = parse(s);
        let arg_span = a.arg_span.unwrap();
        assert_eq!(&s[arg_span.start..arg_span.end], "\"use new\"");
    }
}
