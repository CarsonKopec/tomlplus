//! Pretty-printer / round-trip serializer.

use crate::annotation::{Annotation, AnnotationArg};
use crate::parser::Document;
use crate::value::Value;

pub fn dumps(doc: &Document) -> String {
    let mut out = String::new();

    if !doc.vars.is_empty() {
        out.push_str("[vars]\n");
        for (k, v) in &doc.vars {
            out.push_str(&format!("{} = {}\n", k, format_value(v, 0)));
        }
        out.push('\n');
    }

    // Top-level non-dict keys
    let mut wrote_top = false;
    for (k, v) in &doc.config {
        if !matches!(v, Value::Dict(_)) {
            if let Some(anns) = doc.meta.get(k) {
                emit_annotations(&mut out, anns, "");
            }
            out.push_str(&format!("{} = {}\n", k, format_value(v, 0)));
            wrote_top = true;
        }
    }
    if wrote_top {
        out.push('\n');
    }

    // Sections (top-level dicts)
    for (k, v) in &doc.config {
        if let Value::Dict(d) = v {
            if let Some(anns) = doc.meta.get(k) {
                emit_annotations(&mut out, anns, "");
            }
            out.push_str(&format!("[{}]\n", k));
            for (kk, vv) in d {
                let fqk = format!("{}.{}", k, kk);
                if let Some(anns) = doc.meta.get(&fqk) {
                    emit_annotations(&mut out, anns, "");
                }
                if let Value::Dict(inner) = vv {
                    out.push_str(&format!("{} = #{{\n", kk));
                    for (ik, iv) in inner {
                        let ifqk = format!("{}.{}", fqk, ik);
                        if let Some(anns) = doc.meta.get(&ifqk) {
                            emit_annotations(&mut out, anns, "  ");
                        }
                        out.push_str(&format!("  {} = {}\n", ik, format_value(iv, 1)));
                    }
                    out.push_str("}#\n");
                } else {
                    out.push_str(&format!("{} = {}\n", kk, format_value(vv, 0)));
                }
            }
            out.push('\n');
        }
    }

    out.trim_end().to_string() + "\n"
}

fn emit_annotations(out: &mut String, anns: &[Annotation], indent: &str) {
    for a in anns {
        out.push_str(indent);
        out.push_str(&format_annotation(a));
        out.push('\n');
    }
}

fn format_annotation(a: &Annotation) -> String {
    match &a.arg {
        AnnotationArg::None => format!("@{}", a.name),
        AnnotationArg::String(s) => format!("@{}: {}", a.name, s),
        AnnotationArg::Int(n) => format!("@{}: {}", a.name, n),
        AnnotationArg::Float(f) => format!("@{}: {}", a.name, f),
        AnnotationArg::List(xs) => format!("@{}: [{}]", a.name, xs.join(", ")),
    }
}

fn format_value(v: &Value, _depth: usize) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => {
            if *b {
                "true".into()
            } else {
                "false".into()
            }
        }
        Value::Integer(n) => n.to_string(),
        Value::Float(f) => format_float(*f),
        Value::String(s) => quote(s),
        Value::Array(xs) => {
            let inner: Vec<String> = xs.iter().map(|v| format_value(v, 0)).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Dict(d) => {
            let inner: Vec<String> = d
                .iter()
                .map(|(k, v)| format!("{} = {}", k, format_value(v, 0)))
                .collect();
            format!("#{{ {} }}#", inner.join(", "))
        }
    }
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn format_float(f: f64) -> String {
    if f.fract() == 0.0 && f.is_finite() {
        format!("{:.1}", f)
    } else {
        f.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn block_dict_uses_correct_open() {
        let d = parse("[s]\nh = #{\n  a = 1\n}#");
        let out = dumps(&d);
        assert!(out.contains("#{"));
        assert!(out.contains("}#"));
        assert!(!out.contains("{#"));
    }

    #[test]
    fn inline_dict_close_pound() {
        let mut d: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();
        d.insert("a".to_string(), Value::Integer(1));
        let v = Value::Dict(d);
        let s = format_value(&v, 0);
        assert!(s.ends_with("}#"));
    }
}
