//! Document parser.
//!
//! Two passes:
//!     1. Walk `[vars]` to populate user-defined variables (so they can be
//!        referenced before their definition site doesn't matter).
//!     2. Walk the whole document, attaching pending annotations to the
//!        next key/section, materialising nested sections, accumulating
//!        block-dict bodies.

use std::collections::{BTreeMap, HashMap};

use crate::annotation::{parse_annotation, Annotation};
use crate::error::{Diagnostic, DiagnosticCode};
use crate::lexer::{split_section_path, tokenize, LineKind, LineToken};
use crate::span::Span;
use crate::value::Value;
use crate::value_parser::{ValueParser, VarRef};

#[derive(Debug, Default)]
pub struct Document {
    /// The fully parsed top-level dict.
    pub config: BTreeMap<String, Value>,
    /// Annotations indexed by dotted key path (e.g. `"server.port"`).
    pub meta: HashMap<String, Vec<Annotation>>,
    /// User-defined `[vars]` entries.
    pub vars: BTreeMap<String, Value>,
    /// Span of each defined dotted key path (for goto-def, hover).
    pub key_spans: HashMap<String, Span>,
    /// Span of each key's value expression (for inlay hints + semantic tokens).
    pub value_spans: HashMap<String, Span>,
    /// Span of each `[vars]` entry's key (for goto-def of `$var`).
    pub var_def_spans: HashMap<String, Span>,
    /// Span of each `[vars]` entry's value (for inlay hints over var refs).
    pub var_value_spans: HashMap<String, Span>,
    /// Every `$variable` reference site in the document.
    pub var_refs: Vec<VarRef>,
    /// Diagnostics collected during parsing.
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse(source: &str) -> Document {
    parse_with_env(source, &|k| std::env::var(k).ok())
}

pub fn parse_with_env(source: &str, env: &dyn Fn(&str) -> Option<String>) -> Document {
    let mut doc = Document::default();
    let tokens = tokenize(source);

    pass1_vars(&tokens, &mut doc, env);
    pass2_main(&tokens, &mut doc, env);

    doc
}

fn pass1_vars(tokens: &[LineToken], doc: &mut Document, env: &dyn Fn(&str) -> Option<String>) {
    let mut in_vars = false;

    for tok in tokens {
        match tok.kind {
            LineKind::Vars => {
                in_vars = true;
                continue;
            }
            LineKind::Section => {
                in_vars = false;
                continue;
            }
            _ => {}
        }
        if !in_vars {
            continue;
        }
        if tok.kind == LineKind::Kv {
            let mut vp = ValueParser::new(&doc.vars, env, tok.value_span);
            let result = vp.parse(&tok.value);
            let refs = std::mem::take(&mut vp.var_refs);
            drop(vp);
            match result {
                Ok(v) => {
                    doc.var_def_spans.insert(tok.key.clone(), tok.key_span);
                    doc.var_value_spans.insert(tok.key.clone(), tok.value_span);
                    doc.vars.insert(tok.key.clone(), v);
                    doc.var_refs.extend(refs);
                }
                Err(e) => {
                    let code = classify_value_error(&e);
                    doc.diagnostics.push(e.into_diagnostic(code));
                }
            }
        }
    }
}

fn pass2_main(tokens: &[LineToken], doc: &mut Document, env: &dyn Fn(&str) -> Option<String>) {
    let mut in_vars = false;
    let mut current_section: Vec<String> = Vec::new();
    let mut pending: Vec<Annotation> = Vec::new();

    let mut in_block = false;
    let mut block_key: String = String::new();
    let mut block_data: BTreeMap<String, Value> = BTreeMap::new();
    let mut block_pending: Vec<Annotation> = Vec::new();
    let mut block_ann_snapshot: Vec<Annotation> = Vec::new();
    let mut block_key_span = Span::DUMMY;

    for tok in tokens {
        if in_block {
            handle_block(
                tok,
                doc,
                env,
                &current_section,
                &block_key,
                &block_key_span,
                &mut block_data,
                &mut block_pending,
                &mut block_ann_snapshot,
                &mut in_block,
            );
            continue;
        }

        match tok.kind {
            LineKind::Vars => {
                in_vars = true;
                continue;
            }
            LineKind::Section => in_vars = false,
            LineKind::Blank => continue,
            _ => {}
        }

        if in_vars && !matches!(tok.kind, LineKind::Section) {
            continue;
        }

        match tok.kind {
            LineKind::Annotation => {
                match parse_annotation(&tok.annotation_text, annotation_span(tok)) {
                    Ok(a) => pending.push(a),
                    Err(e) => doc
                        .diagnostics
                        .push(e.into_diagnostic(DiagnosticCode::BadAnnotation)),
                }
            }
            LineKind::Section => {
                let path = split_section_path(&tok.section);
                if path.is_empty() || path.iter().any(|p| p.is_empty()) {
                    doc.diagnostics.push(Diagnostic::error(
                        format!("invalid section name: `{}`", tok.section),
                        tok.section_span,
                        DiagnosticCode::Syntax,
                    ));
                    continue;
                }
                materialize_section(&mut doc.config, &path);
                if !pending.is_empty() {
                    let key = path.join(".");
                    doc.meta.insert(key, std::mem::take(&mut pending));
                }
                doc.key_spans.insert(path.join("."), tok.section_span);
                current_section = path;
            }
            LineKind::BlockOpen => {
                in_block = true;
                block_key = tok.key.clone();
                block_key_span = tok.key_span;
                block_data = BTreeMap::new();
                block_pending = Vec::new();
                block_ann_snapshot = std::mem::take(&mut pending);
            }
            LineKind::Kv => {
                let mut vp = ValueParser::new(&doc.vars, env, tok.value_span);
                let result = vp.parse(&tok.value);
                let refs = std::mem::take(&mut vp.var_refs);
                drop(vp);
                match result {
                    Ok(v) => {
                        store(
                            doc,
                            &current_section,
                            &tok.key,
                            tok.key_span,
                            tok.value_span,
                            v,
                            std::mem::take(&mut pending),
                        );
                        doc.var_refs.extend(refs);
                    }
                    Err(e) => {
                        let code = classify_value_error(&e);
                        doc.diagnostics.push(e.into_diagnostic(code));
                        pending.clear();
                    }
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_block(
    tok: &LineToken,
    doc: &mut Document,
    env: &dyn Fn(&str) -> Option<String>,
    current_section: &[String],
    block_key: &str,
    block_key_span: &Span,
    block_data: &mut BTreeMap<String, Value>,
    block_pending: &mut Vec<Annotation>,
    block_ann_snapshot: &mut Vec<Annotation>,
    in_block: &mut bool,
) {
    match tok.kind {
        LineKind::BlockClose => {
            store(
                doc,
                current_section,
                block_key,
                *block_key_span,
                *block_key_span, // best-effort: full block span tracked later if needed
                Value::Dict(std::mem::take(block_data)),
                std::mem::take(block_ann_snapshot),
            );
            block_pending.clear();
            *in_block = false;
        }
        LineKind::Annotation => {
            match parse_annotation(&tok.annotation_text, annotation_span(tok)) {
                Ok(a) => block_pending.push(a),
                Err(e) => doc
                    .diagnostics
                    .push(e.into_diagnostic(DiagnosticCode::BadAnnotation)),
            }
        }
        LineKind::Kv => {
            // Allow trailing comma so the same body works as block-or-inline.
            let mut value_src: &str = tok.value.trim_end();
            if let Some(stripped) = value_src.strip_suffix(',') {
                value_src = stripped.trim_end();
            }
            let mut vp = ValueParser::new(&doc.vars, env, tok.value_span);
            let result = vp.parse(value_src);
            let refs = std::mem::take(&mut vp.var_refs);
            drop(vp);
            match result {
                Ok(v) => {
                    block_data.insert(tok.key.clone(), v);

                    // Always record key + value spans for every block entry so the
                    // LSP can attach hover, inlay hints, semantic tokens, etc.
                    let key = if current_section.is_empty() {
                        format!("{}.{}", block_key, tok.key)
                    } else {
                        format!("{}.{}.{}", current_section.join("."), block_key, tok.key)
                    };
                    doc.key_spans.insert(key.clone(), tok.key_span);
                    doc.value_spans.insert(key.clone(), tok.value_span);

                    if !block_pending.is_empty() {
                        doc.meta.insert(key, std::mem::take(block_pending));
                    }
                    doc.var_refs.extend(refs);
                }
                Err(e) => {
                    let code = classify_value_error(&e);
                    doc.diagnostics.push(e.into_diagnostic(code));
                    block_pending.clear();
                }
            }
        }
        _ => {}
    }
}

/// Map a value-parser error to a diagnostic code. We sniff the message for
/// the "undefined variable" pattern emitted by the value parser — enough to
/// drive Python's `VariableError` subclass.
fn classify_value_error(e: &crate::error::ParseError) -> DiagnosticCode {
    let msg = match e {
        crate::error::ParseError::Generic { message, .. } => message.as_str(),
    };
    if msg.starts_with("undefined variable") {
        DiagnosticCode::UndefinedVariable
    } else {
        DiagnosticCode::Syntax
    }
}

/// Tight span over the `@name…` portion of an annotation line (excludes
/// leading whitespace, which would otherwise throw off all of the sub-spans
/// computed by [`parse_annotation`]).
fn annotation_span(tok: &LineToken) -> Span {
    let at_offset_in_raw = tok.raw.find('@').unwrap_or(0);
    let start = tok.span.start + at_offset_in_raw;
    Span::new(start, start + tok.annotation_text.len())
}

fn materialize_section(config: &mut BTreeMap<String, Value>, path: &[String]) {
    if path.is_empty() {
        return;
    }
    let mut node = config;
    for part in path {
        let entry = node
            .entry(part.clone())
            .or_insert_with(|| Value::Dict(BTreeMap::new()));
        if !matches!(entry, Value::Dict(_)) {
            // Collision — overwrite to a fresh dict so we can keep walking.
            *entry = Value::Dict(BTreeMap::new());
        }
        node = match entry {
            Value::Dict(d) => d,
            _ => unreachable!(),
        };
    }
}

#[allow(clippy::too_many_arguments)]
fn store(
    doc: &mut Document,
    section: &[String],
    key: &str,
    key_span: Span,
    value_span: Span,
    value: Value,
    annotations: Vec<Annotation>,
) {
    let fqk: String = if section.is_empty() {
        key.to_string()
    } else {
        let mut s = section.join(".");
        s.push('.');
        s.push_str(key);
        s
    };

    if section.is_empty() {
        doc.config.insert(key.to_string(), value);
    } else {
        let mut node = &mut doc.config;
        for part in section {
            let entry = node
                .entry(part.clone())
                .or_insert_with(|| Value::Dict(BTreeMap::new()));
            if !matches!(entry, Value::Dict(_)) {
                *entry = Value::Dict(BTreeMap::new());
            }
            node = match entry {
                Value::Dict(d) => d,
                _ => unreachable!(),
            };
        }
        node.insert(key.to_string(), value);
    }

    doc.key_spans.insert(fqk.clone(), key_span);
    doc.value_spans.insert(fqk.clone(), value_span);
    if !annotations.is_empty() {
        doc.meta.insert(fqk, annotations);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(s: &str) -> Document {
        parse_with_env(s, &|_| None)
    }

    #[test]
    fn simple_kv() {
        let d = parse_str("port = 8080");
        assert_eq!(d.config.get("port"), Some(&Value::Integer(8080)));
        assert!(d.diagnostics.is_empty());
    }

    #[test]
    fn section_kv() {
        let d = parse_str("[server]\nport = 80");
        let server = match d.config.get("server").unwrap() {
            Value::Dict(d) => d,
            _ => panic!(),
        };
        assert_eq!(server.get("port"), Some(&Value::Integer(80)));
    }

    #[test]
    fn dotted_section() {
        let d = parse_str("[a.b.c]\nx = 1");
        let a = match d.config.get("a").unwrap() {
            Value::Dict(d) => d,
            _ => panic!(),
        };
        let b = match a.get("b").unwrap() {
            Value::Dict(d) => d,
            _ => panic!(),
        };
        let c = match b.get("c").unwrap() {
            Value::Dict(d) => d,
            _ => panic!(),
        };
        assert_eq!(c.get("x"), Some(&Value::Integer(1)));
    }

    #[test]
    fn vars_resolution() {
        let d = parse_str("[vars]\nbase = \"x\"\n[s]\nu = $base");
        let s = match d.config.get("s").unwrap() {
            Value::Dict(d) => d,
            _ => panic!(),
        };
        assert_eq!(s.get("u"), Some(&Value::String("x".to_string())));
    }

    #[test]
    fn annotation_and_meta() {
        let d = parse_str("@type: int\nport = 80");
        let anns = d.meta.get("port").unwrap();
        assert_eq!(anns.len(), 1);
        assert_eq!(anns[0].name, "type");
    }

    #[test]
    fn block_dict_with_annotation() {
        let d = parse_str("[server]\nheaders = #{\n  @required\n  ct = \"json\"\n}#");
        let server = match d.config.get("server").unwrap() {
            Value::Dict(d) => d,
            _ => panic!(),
        };
        let headers = match server.get("headers").unwrap() {
            Value::Dict(d) => d,
            _ => panic!(),
        };
        assert_eq!(headers.get("ct"), Some(&Value::String("json".into())));
        assert!(d.meta.contains_key("server.headers.ct"));
    }

    #[test]
    fn multiline_array() {
        let d = parse_str("tags = [\n  \"a\",\n  \"b\",\n]");
        assert_eq!(
            d.config.get("tags"),
            Some(&Value::Array(vec![
                Value::String("a".into()),
                Value::String("b".into())
            ]))
        );
    }

    #[test]
    fn undefined_var_records_diagnostic() {
        let d = parse_str("x = $UNKNOWN");
        assert!(!d.diagnostics.is_empty());
    }
}
