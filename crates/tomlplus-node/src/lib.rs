//! Node.js bindings for the TOML+ language core.
//!
//! Exposes a [`TomlplusDocument`] class plus top-level `parse`, `validate`,
//! `dumps` functions. Mirrors the Python API.

#![deny(clippy::all)]

use napi::bindgen_prelude::*;
use napi_derive::napi;

use serde_json::{Map as JMap, Number as JNum, Value as JValue};
use std::collections::BTreeMap;

use tomlplus_syntax::{
    annotation::AnnotationArg, dumper, parser, validator, value::Value as TpValue,
    LineIndex, Severity,
};

// ── Top-level parse / validate / dumps ───────────────────────────────────────

/// Parse a TOML+ source string. Throws on a fatal parse error.
#[napi]
pub fn parse(source: String) -> Result<TomlplusDocument> {
    let doc = parser::parse(&source);
    if let Some(first) = doc.diagnostics.iter().find(|d| matches!(d.severity, Severity::Error)) {
        let idx = LineIndex::new(&source);
        let (line, col) = idx.position(first.span.start);
        return Err(Error::new(
            Status::InvalidArg,
            format!("{} (line {}, col {})", first.message, line + 1, col + 1),
        ));
    }
    Ok(TomlplusDocument { doc })
}

/// Run the validator. Throws on the first failing constraint.
#[napi]
pub fn validate(doc: &TomlplusDocument) -> Result<()> {
    let errs = validator::validate(&doc.doc);
    if let Some(first) = errs.iter().find(|d| matches!(d.severity, Severity::Error)) {
        return Err(Error::new(Status::InvalidArg, first.message.clone()));
    }
    Ok(())
}

/// Run the validator. Returns *all* failing constraints as plain objects.
#[napi]
pub fn validate_all(doc: &TomlplusDocument) -> JValue {
    let errs = validator::validate(&doc.doc);
    JValue::Array(
        errs.iter()
            .map(|e| serde_json::json!({
                "message":  e.message,
                "severity": severity_name(e.severity),
                "span":     { "start": e.span.start, "end": e.span.end },
            }))
            .collect(),
    )
}

/// Serialise back to TOML+ text.
#[napi]
pub fn dumps(doc: &TomlplusDocument) -> String {
    dumper::dumps(&doc.doc)
}

// ── TomlplusDocument class ───────────────────────────────────────────────────

#[napi]
pub struct TomlplusDocument {
    doc: parser::Document,
}

#[napi]
impl TomlplusDocument {
    /// Resolve a dotted path; returns `null` when missing.
    #[napi]
    pub fn resolve(&self, key_path: String) -> JValue {
        resolve_dotted(&self.doc.config, &key_path)
            .map(value_to_json)
            .unwrap_or(JValue::Null)
    }

    /// Annotations attached to a dotted key path: `[{name, arg}]`.
    #[napi]
    pub fn annotations(&self, key_path: String) -> JValue {
        let xs: Vec<JValue> = self
            .doc
            .meta
            .get(&key_path)
            .map(|anns| {
                anns.iter()
                    .map(|a| {
                        serde_json::json!({
                            "name": a.name,
                            "arg":  arg_to_json(&a.arg),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        JValue::Array(xs)
    }

    /// Is `name` among the annotations on `keyPath`?
    #[napi(js_name = "hasAnnotation")]
    pub fn has_annotation(&self, key_path: String, name: String) -> bool {
        self.doc
            .meta
            .get(&key_path)
            .map(|a| a.iter().any(|a| a.name == name))
            .unwrap_or(false)
    }

    /// `{ tagName: value, … }` for all `@tag` annotations at this path.
    #[napi]
    pub fn tags(&self, key_path: String) -> JValue {
        let mut out = JMap::new();
        if let Some(anns) = self.doc.meta.get(&key_path) {
            for a in anns {
                if a.name == "tag" {
                    if let AnnotationArg::String(s) = &a.arg {
                        if let Some((k, v)) = s.split_once('=') {
                            out.insert(
                                k.trim().to_string(),
                                JValue::String(v.trim().trim_matches('"').to_string()),
                            );
                        }
                    }
                }
            }
        }
        JValue::Object(out)
    }

    /// All key paths annotated with `@required`.
    #[napi(js_name = "requiredKeys")]
    pub fn required_keys(&self) -> Vec<String> {
        self.doc
            .meta
            .iter()
            .filter(|(_, anns)| anns.iter().any(|a| a.name == "required"))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// `[ [keyPath, message|null] ]` for all `@deprecated` keys.
    #[napi(js_name = "deprecatedKeys")]
    pub fn deprecated_keys(&self) -> JValue {
        let mut out: Vec<JValue> = Vec::new();
        for (k, anns) in &self.doc.meta {
            for a in anns {
                if a.name == "deprecated" {
                    let msg = match &a.arg {
                        AnnotationArg::String(s) => JValue::String(s.clone()),
                        _ => JValue::Null,
                    };
                    out.push(JValue::Array(vec![JValue::String(k.clone()), msg]));
                }
            }
        }
        JValue::Array(out)
    }

    /// `[ [keyPath, value] ]` for keys with a `@tag: <tagName> = "value"`.
    #[napi(js_name = "keysWithTag")]
    pub fn keys_with_tag(&self, tag_name: String) -> JValue {
        let mut out: Vec<JValue> = Vec::new();
        for (k, anns) in &self.doc.meta {
            for a in anns {
                if a.name == "tag" {
                    if let AnnotationArg::String(s) = &a.arg {
                        if let Some((tk, tv)) = s.split_once('=') {
                            if tk.trim() == tag_name {
                                out.push(JValue::Array(vec![
                                    JValue::String(k.clone()),
                                    JValue::String(tv.trim().trim_matches('"').to_string()),
                                ]));
                            }
                        }
                    }
                }
            }
        }
        JValue::Array(out)
    }

    /// Whole config tree.
    #[napi(getter)]
    pub fn config(&self) -> JValue {
        value_to_json(&TpValue::Dict(self.doc.config.clone()))
    }

    /// User-defined `[vars]`.
    #[napi(getter)]
    pub fn vars(&self) -> JValue {
        let mut out = JMap::new();
        for (k, v) in &self.doc.vars {
            out.insert(k.clone(), value_to_json(v));
        }
        JValue::Object(out)
    }

    /// All annotation metadata.
    #[napi(getter)]
    pub fn meta(&self) -> JValue {
        let mut out = JMap::new();
        for (k, anns) in &self.doc.meta {
            let xs: Vec<JValue> = anns
                .iter()
                .map(|a| serde_json::json!({ "name": a.name, "arg": arg_to_json(&a.arg) }))
                .collect();
            out.insert(k.clone(), JValue::Array(xs));
        }
        JValue::Object(out)
    }

    /// Top-level section/key names.
    #[napi]
    pub fn keys(&self) -> Vec<String> {
        self.doc.config.keys().cloned().collect()
    }
}

// ── Conversion helpers ───────────────────────────────────────────────────────

fn resolve_dotted<'a>(
    config: &'a BTreeMap<String, TpValue>,
    dotted: &str,
) -> Option<&'a TpValue> {
    let mut parts = dotted.split('.');
    let first = parts.next()?;
    let mut node: &TpValue = config.get(first)?;
    for part in parts {
        node = match node {
            TpValue::Dict(d) => d.get(part)?,
            _ => return None,
        };
    }
    Some(node)
}

fn value_to_json(v: &TpValue) -> JValue {
    match v {
        TpValue::Null       => JValue::Null,
        TpValue::Bool(b)    => JValue::Bool(*b),
        TpValue::Integer(n) => JValue::Number((*n).into()),
        TpValue::Float(f)   => JNum::from_f64(*f).map(JValue::Number).unwrap_or(JValue::Null),
        TpValue::String(s)  => JValue::String(s.clone()),
        TpValue::Array(xs)  => JValue::Array(xs.iter().map(value_to_json).collect()),
        TpValue::Dict(d) => JValue::Object(
            d.iter().map(|(k, v)| (k.clone(), value_to_json(v))).collect(),
        ),
    }
}

fn arg_to_json(a: &AnnotationArg) -> JValue {
    match a {
        AnnotationArg::None      => JValue::Null,
        AnnotationArg::String(s) => JValue::String(s.clone()),
        AnnotationArg::Int(n)    => JValue::Number((*n).into()),
        AnnotationArg::Float(f)  => JNum::from_f64(*f).map(JValue::Number).unwrap_or(JValue::Null),
        AnnotationArg::List(xs)  => JValue::Array(xs.iter().map(|s| JValue::String(s.clone())).collect()),
    }
}

fn severity_name(s: Severity) -> &'static str {
    match s {
        Severity::Error   => "error",
        Severity::Warning => "warning",
        Severity::Info    => "info",
        Severity::Hint    => "hint",
    }
}
