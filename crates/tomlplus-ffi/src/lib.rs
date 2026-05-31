//! C ABI for the TOML+ language core.
//!
//! Surface area is deliberately tiny: parse a TOML+ string, then ask for any
//! piece of the result as a JSON blob. Every output string is owned by the
//! library; callers must release it with [`tomlplus_free_string`]. Every
//! opaque handle is released with [`tomlplus_free`].
//!
//! Errors are stored thread-locally; check [`tomlplus_last_error`] after any
//! call that returns NULL.
//!
//! ```text
//! TomlplusDoc* doc = tomlplus_parse(source);
//! if (!doc) { fprintf(stderr, "%s\n", tomlplus_last_error()); return 1; }
//! char* json = tomlplus_to_json(doc);
//! puts(json);
//! tomlplus_free_string(json);
//! tomlplus_free(doc);
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::{c_char, CStr, CString};
use std::ptr;

use tomlplus_syntax::{
    annotation::AnnotationArg, dumper, parser, validator, value::Value, LineIndex, Severity,
};

// ── Last-error storage (thread-local) ────────────────────────────────────────

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_last_error<S: Into<String>>(msg: S) {
    let cs =
        CString::new(msg.into()).unwrap_or_else(|_| CString::new("invalid error string").unwrap());
    LAST_ERROR.with(|cell| *cell.borrow_mut() = Some(cs));
}

fn clear_last_error() {
    LAST_ERROR.with(|cell| *cell.borrow_mut() = None);
}

/// Returns the last error message produced on the calling thread, or NULL.
/// The returned pointer is owned by the library; do not free it.
#[no_mangle]
pub extern "C" fn tomlplus_last_error() -> *const c_char {
    LAST_ERROR.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(ptr::null())
    })
}

// ── Opaque handle ────────────────────────────────────────────────────────────

/// Opaque handle to a parsed TOML+ document. Allocate with
/// [`tomlplus_parse`] and free with [`tomlplus_free`].
pub struct TomlplusDoc {
    doc: parser::Document,
}

// ── String / handle freers ───────────────────────────────────────────────────

/// Free a string previously returned by this library. Safe to call with NULL.
///
/// # Safety
/// `s` must have been returned by a `tomlplus_*` function that allocated a
/// fresh string. Passing any other pointer is undefined behaviour.
#[no_mangle]
pub unsafe extern "C" fn tomlplus_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

/// Free a document handle. Safe to call with NULL.
///
/// # Safety
/// `doc` must have been returned by [`tomlplus_parse`] and not previously freed.
#[no_mangle]
pub unsafe extern "C" fn tomlplus_free(doc: *mut TomlplusDoc) {
    if !doc.is_null() {
        drop(unsafe { Box::from_raw(doc) });
    }
}

// ── Parsing ──────────────────────────────────────────────────────────────────

/// Parse a NUL-terminated UTF-8 TOML+ source string.
///
/// On success returns a non-NULL handle. On a fatal parse error, returns NULL
/// and stores a message readable via [`tomlplus_last_error`]. Non-fatal
/// diagnostics (e.g. validator warnings) do not cause NULL.
///
/// # Safety
/// `source` must point to a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tomlplus_parse(source: *const c_char) -> *mut TomlplusDoc {
    clear_last_error();
    if source.is_null() {
        set_last_error("source pointer is null");
        return ptr::null_mut();
    }
    let cstr = unsafe { CStr::from_ptr(source) };
    let text = match cstr.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_last_error(format!("invalid utf-8 in source: {}", e));
            return ptr::null_mut();
        }
    };

    let doc = parser::parse(text);

    if let Some(first) = doc
        .diagnostics
        .iter()
        .find(|d| matches!(d.severity, Severity::Error))
    {
        let idx = LineIndex::new(text);
        let (line, col) = idx.position(first.span.start);
        set_last_error(format!(
            "{} (line {}, col {})",
            first.message,
            line + 1,
            col + 1
        ));
        return ptr::null_mut();
    }

    Box::into_raw(Box::new(TomlplusDoc { doc }))
}

// ── Output accessors ─────────────────────────────────────────────────────────

/// Serialise the parsed config tree as a JSON string. Caller must free.
///
/// # Safety
/// `doc` must be a valid handle returned by [`tomlplus_parse`].
#[no_mangle]
pub unsafe extern "C" fn tomlplus_to_json(doc: *const TomlplusDoc) -> *mut c_char {
    let Some(d) = (unsafe { doc.as_ref() }) else {
        set_last_error("doc pointer is null");
        return ptr::null_mut();
    };
    let json = value_to_json(&Value::Dict(d.doc.config.clone()));
    string_into_c(serde_json::to_string(&json))
}

/// Serialise annotation metadata as `{ "key.path": [ {name, arg} ] }` JSON.
///
/// # Safety
/// `doc` must be a valid handle returned by [`tomlplus_parse`].
#[no_mangle]
pub unsafe extern "C" fn tomlplus_meta_json(doc: *const TomlplusDoc) -> *mut c_char {
    let Some(d) = (unsafe { doc.as_ref() }) else {
        set_last_error("doc pointer is null");
        return ptr::null_mut();
    };
    let mut out = serde_json::Map::new();
    for (k, anns) in &d.doc.meta {
        let entries: Vec<serde_json::Value> = anns
            .iter()
            .map(|a| {
                serde_json::json!({
                    "name": a.name,
                    "arg": arg_to_json(&a.arg),
                })
            })
            .collect();
        out.insert(k.clone(), serde_json::Value::Array(entries));
    }
    string_into_c(serde_json::to_string(&serde_json::Value::Object(out)))
}

/// Serialise `[vars]` as a JSON object.
///
/// # Safety
/// `doc` must be a valid handle returned by [`tomlplus_parse`].
#[no_mangle]
pub unsafe extern "C" fn tomlplus_vars_json(doc: *const TomlplusDoc) -> *mut c_char {
    let Some(d) = (unsafe { doc.as_ref() }) else {
        set_last_error("doc pointer is null");
        return ptr::null_mut();
    };
    let mut out = serde_json::Map::new();
    for (k, v) in &d.doc.vars {
        out.insert(k.clone(), value_to_json(v));
    }
    string_into_c(serde_json::to_string(&serde_json::Value::Object(out)))
}

/// Run the validator. Returns a JSON array of error objects (possibly empty),
/// never NULL on success.
///
/// # Safety
/// `doc` must be a valid handle returned by [`tomlplus_parse`].
#[no_mangle]
pub unsafe extern "C" fn tomlplus_validate(doc: *const TomlplusDoc) -> *mut c_char {
    let Some(d) = (unsafe { doc.as_ref() }) else {
        set_last_error("doc pointer is null");
        return ptr::null_mut();
    };
    let errs = validator::validate(&d.doc);
    let mut out = Vec::with_capacity(errs.len());
    for e in errs {
        out.push(serde_json::json!({
            "message": e.message,
            "severity": severity_name(e.severity),
            "span": { "start": e.span.start, "end": e.span.end },
        }));
    }
    string_into_c(serde_json::to_string(&serde_json::Value::Array(out)))
}

/// Re-serialise the document back to TOML+ text. Caller must free.
///
/// # Safety
/// `doc` must be a valid handle returned by [`tomlplus_parse`].
#[no_mangle]
pub unsafe extern "C" fn tomlplus_dumps(doc: *const TomlplusDoc) -> *mut c_char {
    let Some(d) = (unsafe { doc.as_ref() }) else {
        set_last_error("doc pointer is null");
        return ptr::null_mut();
    };
    string_into_c(Ok::<String, std::convert::Infallible>(dumper::dumps(
        &d.doc,
    )))
}

/// Library version (CARGO_PKG_VERSION). Pointer is static; do not free.
#[no_mangle]
pub extern "C" fn tomlplus_version() -> *const c_char {
    static VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
    VERSION.as_ptr() as *const c_char
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn string_into_c<E: std::fmt::Display>(r: Result<String, E>) -> *mut c_char {
    match r.and_then(|s| CString::new(s).map_err(|e| panic!("nul in output: {e}"))) {
        Ok(cs) => cs.into_raw(),
        Err(e) => {
            set_last_error(format!("serialisation failed: {}", e));
            ptr::null_mut()
        }
    }
}

fn value_to_json(v: &Value) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        Value::Null => J::Null,
        Value::Bool(b) => J::Bool(*b),
        Value::Integer(n) => J::Number((*n).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(J::Number)
            .unwrap_or(J::Null),
        Value::String(s) => J::String(s.clone()),
        Value::Array(xs) => J::Array(xs.iter().map(value_to_json).collect()),
        Value::Dict(d) => J::Object(
            d.iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect(),
        ),
    }
}

fn arg_to_json(a: &AnnotationArg) -> serde_json::Value {
    match a {
        AnnotationArg::None => serde_json::Value::Null,
        AnnotationArg::String(s) => serde_json::Value::String(s.clone()),
        AnnotationArg::Int(n) => serde_json::Value::Number((*n).into()),
        AnnotationArg::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        AnnotationArg::List(xs) => serde_json::Value::Array(
            xs.iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
        ),
    }
}

fn severity_name(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    }
}

// keep BTreeMap referenced so we don't drop the dependency
#[allow(dead_code)]
fn _keep_btree(_m: BTreeMap<String, Value>) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> *mut TomlplusDoc {
        let c = CString::new(src).unwrap();
        unsafe { tomlplus_parse(c.as_ptr()) }
    }

    fn take_string(p: *mut c_char) -> String {
        unsafe {
            let s = CStr::from_ptr(p).to_str().unwrap().to_string();
            tomlplus_free_string(p);
            s
        }
    }

    #[test]
    fn parse_and_to_json() {
        let doc = parse("[server]\nport = 8080");
        assert!(!doc.is_null());
        let json = unsafe { tomlplus_to_json(doc) };
        let s = take_string(json);
        assert!(s.contains("\"port\":8080"));
        unsafe { tomlplus_free(doc) };
    }

    #[test]
    fn parse_error_sets_last_error() {
        let doc = parse("x = $UNDEFINED");
        assert!(doc.is_null());
        let p = tomlplus_last_error();
        assert!(!p.is_null());
        let msg = unsafe { CStr::from_ptr(p).to_str().unwrap() };
        assert!(msg.contains("undefined variable"));
    }

    #[test]
    fn validate_returns_empty_array_when_ok() {
        let doc = parse("@min: 1\nport = 80");
        let json = unsafe { tomlplus_validate(doc) };
        assert_eq!(take_string(json), "[]");
        unsafe { tomlplus_free(doc) };
    }

    #[test]
    fn dumps_roundtrip() {
        let doc = parse("[s]\nport = 80");
        let json = unsafe { tomlplus_dumps(doc) };
        let s = take_string(json);
        assert!(s.contains("[s]"));
        unsafe { tomlplus_free(doc) };
    }
}
