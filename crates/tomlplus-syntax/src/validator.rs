//! Annotation-driven validator. Operates on a parsed [`Document`].

use regex_lite::Regex;
use std::collections::HashMap;

use crate::annotation::{Annotation, AnnotationArg};
use crate::error::{Diagnostic, DiagnosticCode};
use crate::parser::Document;
use crate::span::Span;
use crate::value::Value;

/// Annotations whose semantics only fit leaf scalar values. Skipped on dicts.
const LEAF_ONLY: &[&str] = &[
    "type", "min", "max", "pattern", "enum",
    "positive", "nonzero", "minlen", "maxlen",
];

pub fn validate(doc: &Document) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (key, anns) in &doc.meta {
        let value = match resolve(&doc.config, key) {
            Some(v) => v,
            None => continue,
        };
        let span = doc.key_spans.get(key).copied().unwrap_or(Span::DUMMY);
        validate_one(key, value, anns, span, &mut out);
    }
    out
}

fn validate_one(
    key: &str,
    value: &Value,
    annotations: &[Annotation],
    span: Span,
    out: &mut Vec<Diagnostic>,
) {
    let is_dict = matches!(value, Value::Dict(_));
    let mut by_name: HashMap<&str, &Annotation> = HashMap::new();
    for a in annotations {
        if is_dict && LEAF_ONLY.contains(&a.name.as_str()) {
            continue;
        }
        by_name.insert(a.name.as_str(), a);
    }

    if by_name.contains_key("required") && value.is_empty_like() {
        out.push(Diagnostic::error(
            format!("[{}] value is @required but is empty or null", key),
            span,
            DiagnosticCode::Validation,
        ));
    }

    if let Some(a) = by_name.get("type") {
        if let AnnotationArg::String(t) = &a.arg {
            if !type_matches(t, value) {
                out.push(Diagnostic::error(
                    format!(
                        "[{}] expected @type: {}, got {}",
                        key, t, value.type_name()
                    ),
                    span,
                    DiagnosticCode::Validation,
                ));
            }
        }
    }

    if let Some(a) = by_name.get("min") {
        if let Some(n) = arg_as_num(&a.arg) {
            if let Some(v) = value.as_float() {
                if v < n {
                    out.push(Diagnostic::error(
                        format!("[{}] value {} is below @min: {}", key, v, n),
                        span,
                        DiagnosticCode::Validation,
                    ));
                }
            }
        }
    }
    if let Some(a) = by_name.get("max") {
        if let Some(n) = arg_as_num(&a.arg) {
            if let Some(v) = value.as_float() {
                if v > n {
                    out.push(Diagnostic::error(
                        format!("[{}] value {} exceeds @max: {}", key, v, n),
                        span,
                        DiagnosticCode::Validation,
                    ));
                }
            }
        }
    }

    if let Some(a) = by_name.get("minlen") {
        if let Some(n) = arg_as_int(&a.arg) {
            let len = len_of(value);
            if let Some(l) = len {
                if (l as i64) < n {
                    out.push(Diagnostic::error(
                        format!("[{}] length {} is below @minlen: {}", key, l, n),
                        span,
                        DiagnosticCode::Validation,
                    ));
                }
            }
        }
    }
    if let Some(a) = by_name.get("maxlen") {
        if let Some(n) = arg_as_int(&a.arg) {
            let len = len_of(value);
            if let Some(l) = len {
                if (l as i64) > n {
                    out.push(Diagnostic::error(
                        format!("[{}] length {} exceeds @maxlen: {}", key, l, n),
                        span,
                        DiagnosticCode::Validation,
                    ));
                }
            }
        }
    }

    if let Some(a) = by_name.get("pattern") {
        if let AnnotationArg::String(pat) = &a.arg {
            if let Value::String(s) = value {
                match Regex::new(&format!("^(?:{})$", pat)) {
                    Ok(re) if !re.is_match(s) => out.push(Diagnostic::error(
                        format!(
                            "[{}] value `{}` does not match @pattern: `{}`",
                            key, s, pat
                        ),
                        span,
                        DiagnosticCode::Validation,
                    )),
                    Err(e) => out.push(Diagnostic::error(
                        format!("[{}] invalid @pattern regex: {}", key, e),
                        a.span,
                        DiagnosticCode::BadAnnotation,
                    )),
                    _ => {}
                }
            }
        }
    }

    if let Some(a) = by_name.get("enum") {
        if let AnnotationArg::List(choices) = &a.arg {
            if let Value::String(s) = value {
                if !choices.iter().any(|c| c == s) {
                    out.push(Diagnostic::error(
                        format!(
                            "[{}] value `{}` not in @enum: [{}]",
                            key, s, choices.join(", ")
                        ),
                        span,
                        DiagnosticCode::Validation,
                    ));
                }
            }
        }
    }

    if by_name.contains_key("positive") {
        if let Some(v) = value.as_float() {
            if v <= 0.0 {
                out.push(Diagnostic::error(
                    format!("[{}] value {} must be @positive (> 0)", key, v),
                    span,
                    DiagnosticCode::Validation,
                ));
            }
        }
    }

    if by_name.contains_key("nonzero") {
        let zero = matches!(value, Value::Integer(0))
            || matches!(value, Value::Float(f) if *f == 0.0);
        if zero {
            out.push(Diagnostic::error(
                format!("[{}] value must be @nonzero", key),
                span,
                DiagnosticCode::Validation,
            ));
        }
    }

    if by_name.contains_key("nonempty") && value.is_empty_like() {
        out.push(Diagnostic::error(
            format!("[{}] value must be @nonempty", key),
            span,
            DiagnosticCode::Validation,
        ));
    }

    if let Some(a) = by_name.get("deprecated") {
        let msg = match &a.arg {
            AnnotationArg::String(s) => format!("`{}` is deprecated: {}", key, s),
            _ => format!("`{}` is deprecated", key),
        };
        out.push(Diagnostic::warning(msg, span, DiagnosticCode::Deprecated));
    }
}

fn type_matches(t: &str, v: &Value) -> bool {
    match t {
        "string"      => matches!(v, Value::String(_)),
        "int"         => matches!(v, Value::Integer(_)),
        "float"       => matches!(v, Value::Float(_) | Value::Integer(_)),
        "bool"        => matches!(v, Value::Bool(_)),
        "dict"        => matches!(v, Value::Dict(_)),
        "list"        => matches!(v, Value::Array(_)),
        "list[string]" => list_of(v, |e| matches!(e, Value::String(_))),
        "list[int]"    => list_of(v, |e| matches!(e, Value::Integer(_))),
        "list[float]"  => list_of(v, |e| matches!(e, Value::Float(_) | Value::Integer(_))),
        "list[bool]"   => list_of(v, |e| matches!(e, Value::Bool(_))),
        "url"   => matches!(v, Value::String(s) if is_url(s)),
        "email" => matches!(v, Value::String(s) if is_email(s)),
        "path"  => matches!(v, Value::String(_)),
        "duration" => matches!(v, Value::String(s) if is_duration(s)),
        _ => true,  // unknown type names pass silently, matching Python
    }
}

fn list_of(v: &Value, pred: impl Fn(&Value) -> bool) -> bool {
    matches!(v, Value::Array(items) if items.iter().all(pred))
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

fn is_email(s: &str) -> bool {
    let mut at = 0;
    let mut dot_after_at = false;
    for (i, c) in s.char_indices() {
        if c == '@' {
            at += 1;
            if i == 0 {
                return false;
            }
        }
        if at >= 1 && c == '.' && i > 0 {
            dot_after_at = true;
        }
        if c.is_whitespace() {
            return false;
        }
    }
    at == 1 && dot_after_at && !s.ends_with('.') && !s.ends_with('@')
}

fn is_duration(s: &str) -> bool {
    if s.len() < 2 {
        return false;
    }
    let (num, suf) = s.split_at(s.len() - 1);
    matches!(suf, "s" | "m" | "h" | "d") && num.chars().all(|c| c.is_ascii_digit())
}

fn arg_as_num(a: &AnnotationArg) -> Option<f64> {
    match a {
        AnnotationArg::Int(n) => Some(*n as f64),
        AnnotationArg::Float(f) => Some(*f),
        _ => None,
    }
}

fn arg_as_int(a: &AnnotationArg) -> Option<i64> {
    match a {
        AnnotationArg::Int(n) => Some(*n),
        AnnotationArg::Float(f) => Some(*f as i64),
        _ => None,
    }
}

fn len_of(v: &Value) -> Option<usize> {
    match v {
        Value::String(s) => Some(s.chars().count()),
        Value::Array(a)  => Some(a.len()),
        Value::Dict(d)   => Some(d.len()),
        _ => None,
    }
}

fn resolve<'a>(
    config: &'a std::collections::BTreeMap<String, Value>,
    dotted: &str,
) -> Option<&'a Value> {
    let mut parts = dotted.split('.');
    let first = parts.next()?;
    let mut node: &Value = config.get(first)?;
    for part in parts {
        node = match node {
            Value::Dict(d) => d.get(part)?,
            _ => return None,
        };
    }
    Some(node)
}
