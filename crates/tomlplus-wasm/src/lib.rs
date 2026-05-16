//! WebAssembly bindings for the TOML+ language core.
//!
//! Produces a single `.wasm` + JS wrapper that runs anywhere with a WASM
//! host: browsers, Node, Deno, Bun, Cloudflare Workers, etc.
//!
//! The public JS surface mirrors `tomlplus-node` so application code can
//! choose between the native and the WASM build by swapping a single
//! `import` statement.

#![allow(clippy::module_name_repetitions)]

use std::collections::BTreeMap;

use wasm_bindgen::prelude::*;

use tomlplus_syntax::{
    annotation::AnnotationArg,
    dumper, parser, validator,
    value::Value,
    LineIndex, Severity,
};

// ── Panic hook (better browser-console errors) ───────────────────────────────

#[wasm_bindgen(start)]
pub fn _on_load() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ── Top-level functions ──────────────────────────────────────────────────────

/// Parse a TOML+ source string. Throws on a fatal parse error.
#[wasm_bindgen]
pub fn parse(source: &str) -> Result<TomlplusDocument, JsError> {
    let doc = parser::parse(source);
    if let Some(first) = doc
        .diagnostics
        .iter()
        .find(|d| matches!(d.severity, Severity::Error))
    {
        let idx = LineIndex::new(source);
        let (line, col) = idx.position(first.span.start);
        return Err(JsError::new(&format!(
            "{} (line {}, col {})",
            first.message,
            line + 1,
            col + 1
        )));
    }
    Ok(TomlplusDocument { doc })
}

/// Run the validator. Throws on the first failing constraint.
#[wasm_bindgen]
pub fn validate(doc: &TomlplusDocument) -> Result<(), JsError> {
    if let Some(first) = validator::validate(&doc.doc)
        .into_iter()
        .find(|d| matches!(d.severity, Severity::Error))
    {
        return Err(JsError::new(&first.message));
    }
    Ok(())
}

/// Run the validator. Returns every failing constraint as a plain JS array.
#[wasm_bindgen(js_name = validateAll)]
pub fn validate_all(doc: &TomlplusDocument) -> Result<JsValue, JsError> {
    let errs: Vec<serde_json::Value> = validator::validate(&doc.doc)
        .into_iter()
        .map(|d| {
            serde_json::json!({
                "message":  d.message,
                "severity": severity_name(d.severity),
                "span":     { "start": d.span.start, "end": d.span.end },
            })
        })
        .collect();
    serde_wasm_bindgen::to_value(&errs).map_err(|e| JsError::new(&e.to_string()))
}

/// Re-serialise the document back to TOML+ text.
#[wasm_bindgen]
pub fn dumps(doc: &TomlplusDocument) -> String {
    dumper::dumps(&doc.doc)
}

// ── TomlplusDocument class ───────────────────────────────────────────────────

#[wasm_bindgen]
pub struct TomlplusDocument {
    doc: parser::Document,
}

#[wasm_bindgen]
impl TomlplusDocument {
    /// Walk a dotted path; returns `null` when missing.
    #[wasm_bindgen]
    pub fn resolve(&self, key_path: &str) -> Result<JsValue, JsError> {
        let v = resolve_dotted(&self.doc.config, key_path);
        match v {
            Some(v) => to_js_via_json(&value_to_json(v)),
            None => Ok(JsValue::NULL),
        }
    }

    /// Annotations attached to a dotted key path: `[{name, arg}, …]`.
    #[wasm_bindgen]
    pub fn annotations(&self, key_path: &str) -> Result<JsValue, JsError> {
        let xs: Vec<serde_json::Value> = self
            .doc
            .meta
            .get(key_path)
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
        to_js(&xs).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Is the named annotation present at `keyPath`?
    #[wasm_bindgen(js_name = hasAnnotation)]
    pub fn has_annotation(&self, key_path: &str, name: &str) -> bool {
        self.doc
            .meta
            .get(key_path)
            .map(|a| a.iter().any(|a| a.name == name))
            .unwrap_or(false)
    }

    /// `{ tagName: value, … }` for all `@tag` annotations at this path.
    #[wasm_bindgen]
    pub fn tags(&self, key_path: &str) -> Result<JsValue, JsError> {
        let mut out = serde_json::Map::new();
        if let Some(anns) = self.doc.meta.get(key_path) {
            for a in anns {
                if a.name == "tag" {
                    if let AnnotationArg::String(s) = &a.arg {
                        if let Some((k, v)) = s.split_once('=') {
                            out.insert(
                                k.trim().to_string(),
                                serde_json::Value::String(v.trim().trim_matches('"').to_string()),
                            );
                        }
                    }
                }
            }
        }
        to_js(&serde_json::Value::Object(out))
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// All key paths annotated with `@required`.
    #[wasm_bindgen(js_name = requiredKeys)]
    pub fn required_keys(&self) -> Vec<String> {
        self.doc
            .meta
            .iter()
            .filter(|(_, anns)| anns.iter().any(|a| a.name == "required"))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// `[[keyPath, message|null]]` for all `@deprecated` keys.
    #[wasm_bindgen(js_name = deprecatedKeys)]
    pub fn deprecated_keys(&self) -> Result<JsValue, JsError> {
        let mut out: Vec<serde_json::Value> = Vec::new();
        for (k, anns) in &self.doc.meta {
            for a in anns {
                if a.name == "deprecated" {
                    let msg = match &a.arg {
                        AnnotationArg::String(s) => serde_json::Value::String(s.clone()),
                        _ => serde_json::Value::Null,
                    };
                    out.push(serde_json::json!([k, msg]));
                }
            }
        }
        to_js(&out).map_err(|e| JsError::new(&e.to_string()))
    }

    /// `[[keyPath, value]]` for keys carrying a `@tag: <tagName> = "value"`.
    #[wasm_bindgen(js_name = keysWithTag)]
    pub fn keys_with_tag(&self, tag_name: &str) -> Result<JsValue, JsError> {
        let mut out: Vec<serde_json::Value> = Vec::new();
        for (k, anns) in &self.doc.meta {
            for a in anns {
                if a.name == "tag" {
                    if let AnnotationArg::String(s) = &a.arg {
                        if let Some((tk, tv)) = s.split_once('=') {
                            if tk.trim() == tag_name {
                                out.push(serde_json::json!([
                                    k,
                                    tv.trim().trim_matches('"').to_string()
                                ]));
                            }
                        }
                    }
                }
            }
        }
        to_js(&out).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Whole config tree as a JS object.
    #[wasm_bindgen(getter)]
    pub fn config(&self) -> Result<JsValue, JsError> {
        // Bridge through JSON so `null` values survive (serde-wasm-bindgen
        // serializes Rust unit/Null as `undefined`, which JSON.stringify
        // then drops on the JS side).
        to_js_via_json(&value_to_json(&Value::Dict(self.doc.config.clone())))
    }

    /// User-defined `[vars]` entries.
    #[wasm_bindgen(getter)]
    pub fn vars(&self) -> Result<JsValue, JsError> {
        let mut out = serde_json::Map::new();
        for (k, v) in &self.doc.vars {
            out.insert(k.clone(), value_to_json(v));
        }
        to_js_via_json(&serde_json::Value::Object(out))
    }

    /// All annotation metadata, keyed by dotted path.
    #[wasm_bindgen(getter)]
    pub fn meta(&self) -> Result<JsValue, JsError> {
        let mut out = serde_json::Map::new();
        for (k, anns) in &self.doc.meta {
            let xs: Vec<serde_json::Value> = anns
                .iter()
                .map(|a| serde_json::json!({ "name": a.name, "arg": arg_to_json(&a.arg) }))
                .collect();
            out.insert(k.clone(), serde_json::Value::Array(xs));
        }
        to_js(&serde_json::Value::Object(out))
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Top-level section/key names.
    #[wasm_bindgen]
    pub fn keys(&self) -> Vec<String> {
        self.doc.config.keys().cloned().collect()
    }
}

// ── Conversion helpers ───────────────────────────────────────────────────────

fn resolve_dotted<'a>(
    config: &'a BTreeMap<String, Value>,
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

fn value_to_json(v: &Value) -> serde_json::Value {
    use serde_json::{Number, Value as J};
    match v {
        Value::Null       => J::Null,
        Value::Bool(b)    => J::Bool(*b),
        Value::Integer(n) => J::Number((*n).into()),
        Value::Float(f)   => Number::from_f64(*f).map(J::Number).unwrap_or(J::Null),
        Value::String(s)  => J::String(s.clone()),
        Value::Array(xs)  => J::Array(xs.iter().map(value_to_json).collect()),
        Value::Dict(d)    => J::Object(
            d.iter().map(|(k, v)| (k.clone(), value_to_json(v))).collect(),
        ),
    }
}

fn arg_to_json(a: &AnnotationArg) -> serde_json::Value {
    use serde_json::{Number, Value as J};
    match a {
        AnnotationArg::None      => J::Null,
        AnnotationArg::String(s) => J::String(s.clone()),
        AnnotationArg::Int(n)    => J::Number((*n).into()),
        AnnotationArg::Float(f)  => Number::from_f64(*f).map(J::Number).unwrap_or(J::Null),
        AnnotationArg::List(xs)  => J::Array(xs.iter().map(|s| J::String(s.clone())).collect()),
    }
}

/// Serialize a serde value to JS, producing plain `Object`s (not `Map`s).
/// Without this, callers would have to write `a.get("name")` instead of
/// the idiomatic `a.name`.
fn to_js<T: serde::Serialize + ?Sized>(value: &T) -> Result<JsValue, serde_wasm_bindgen::Error> {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    value.serialize(&serializer)
}

/// Serialize a serde value to JS via `JSON.parse(serde_json::to_string(…))`.
/// Slower than `to_js` but preserves `null` (which `serde-wasm-bindgen`
/// serializes as `undefined`, then `JSON.stringify` drops on the caller side).
fn to_js_via_json<T: serde::Serialize + ?Sized>(value: &T) -> Result<JsValue, JsError> {
    let s = serde_json::to_string(value).map_err(|e| JsError::new(&e.to_string()))?;
    js_sys::JSON::parse(&s).map_err(|e| JsError::new(&format!("JSON.parse failed: {:?}", e)))
}

fn severity_name(s: Severity) -> &'static str {
    match s {
        Severity::Error   => "error",
        Severity::Warning => "warning",
        Severity::Info    => "info",
        Severity::Hint    => "hint",
    }
}
