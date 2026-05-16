//! Python bindings for the TOML+ language core.
//!
//! Surfaces parallel the existing pure-Python `tomlplus` package:
//!     loads / load / loads_validated / load_validated
//!     dumps
//!     validate / validate_all
//!     TOMLPlusDocument, Annotation
//!     TOMLPlusError, ParseError, ValidationError, VariableError
//!
//! The thin Python wrapper at `python/tomlplus/__init__.py` re-exports these
//! and adds the file-loading entry points that need stdlib IO.

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple};

use std::collections::BTreeMap;

use tomlplus_syntax::{
    annotation::{Annotation as TpAnnotation, AnnotationArg},
    dumper, parser, validator,
    value::Value,
    DiagnosticCode, LineIndex, Severity,
};

// ── Exception hierarchy ──────────────────────────────────────────────────────

create_exception!(tomlplus_native, TOMLPlusError, PyException);
create_exception!(tomlplus_native, ParseError, TOMLPlusError);
create_exception!(tomlplus_native, ValidationError, TOMLPlusError);
create_exception!(tomlplus_native, VariableError, TOMLPlusError);

// ── Annotation wrapper ───────────────────────────────────────────────────────

#[pyclass(module = "tomlplus._native", name = "Annotation")]
#[derive(Clone)]
struct PyAnnotation {
    inner: TpAnnotation,
}

#[pymethods]
impl PyAnnotation {
    #[new]
    #[pyo3(signature = (name, arg=None))]
    fn new(name: String, arg: Option<PyObject>) -> PyResult<Self> {
        let arg = match arg {
            None => AnnotationArg::None,
            Some(obj) => Python::with_gil(|py| py_to_arg(py, obj))?,
        };
        Ok(Self {
            inner: TpAnnotation {
                name,
                arg,
                span: tomlplus_syntax::Span::DUMMY,
                name_span: tomlplus_syntax::Span::DUMMY,
                arg_span: None,
                list_item_spans: Vec::new(),
            },
        })
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    #[getter]
    fn arg<'py>(&self, py: Python<'py>) -> PyObject {
        arg_to_py(py, &self.inner.arg)
    }

    #[getter]
    fn is_metadata(&self) -> bool {
        self.inner.is_metadata()
    }

    #[getter]
    fn is_validation(&self) -> bool {
        self.inner.is_validation()
    }

    #[getter]
    fn is_type_hint(&self) -> bool {
        self.inner.is_type()
    }

    #[getter]
    fn is_tag(&self) -> bool {
        self.inner.is_tag()
    }

    fn __repr__(&self) -> String {
        format!("Annotation({:?}, {})", self.inner.name, arg_repr(&self.inner.arg))
    }

    fn __str__(&self) -> String {
        match &self.inner.arg {
            AnnotationArg::None      => format!("@{}", self.inner.name),
            AnnotationArg::String(s) => format!("@{}: {}", self.inner.name, s),
            AnnotationArg::Int(n)    => format!("@{}: {}", self.inner.name, n),
            AnnotationArg::Float(f)  => format!("@{}: {}", self.inner.name, f),
            AnnotationArg::List(xs)  => format!("@{}: [{}]", self.inner.name, xs.join(", ")),
        }
    }

    fn __eq__(&self, other: &PyAnnotation) -> bool {
        self.inner.name == other.inner.name && self.inner.arg == other.inner.arg
    }
}

fn arg_repr(a: &AnnotationArg) -> String {
    match a {
        AnnotationArg::None      => "None".to_string(),
        AnnotationArg::String(s) => format!("{:?}", s),
        AnnotationArg::Int(n)    => n.to_string(),
        AnnotationArg::Float(f)  => f.to_string(),
        AnnotationArg::List(xs)  => format!("{:?}", xs),
    }
}

fn arg_to_py(py: Python<'_>, a: &AnnotationArg) -> PyObject {
    match a {
        AnnotationArg::None      => py.None(),
        AnnotationArg::String(s) => s.into_py(py),
        AnnotationArg::Int(n)    => n.into_py(py),
        AnnotationArg::Float(f)  => f.into_py(py),
        AnnotationArg::List(xs)  => xs.clone().into_py(py),
    }
}

fn py_to_arg(py: Python<'_>, obj: PyObject) -> PyResult<AnnotationArg> {
    let any = obj.bind(py);
    if any.is_none() {
        return Ok(AnnotationArg::None);
    }
    if let Ok(s) = any.extract::<String>() {
        return Ok(AnnotationArg::String(s));
    }
    if let Ok(n) = any.extract::<i64>() {
        return Ok(AnnotationArg::Int(n));
    }
    if let Ok(f) = any.extract::<f64>() {
        return Ok(AnnotationArg::Float(f));
    }
    if let Ok(xs) = any.extract::<Vec<String>>() {
        return Ok(AnnotationArg::List(xs));
    }
    Ok(AnnotationArg::String(any.str()?.to_string()))
}

// ── Document wrapper ─────────────────────────────────────────────────────────

#[pyclass(module = "tomlplus._native", name = "TOMLPlusDocument")]
struct PyDocument {
    doc: parser::Document,
}

#[pymethods]
impl PyDocument {
    fn __getitem__<'py>(&self, py: Python<'py>, key: &str) -> PyResult<PyObject> {
        match self.doc.config.get(key) {
            Some(v) => Ok(value_to_py(py, v)),
            None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_string())),
        }
    }

    fn __contains__(&self, key: &str) -> bool {
        self.doc.config.contains_key(key)
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyResult<KeyIter> {
        let keys: Vec<String> = slf.doc.config.keys().cloned().collect();
        Ok(KeyIter { keys, idx: 0 })
    }

    fn __len__(&self) -> usize {
        self.doc.config.len()
    }

    fn __repr__(&self) -> String {
        let sections: Vec<String> = self.doc.config.keys().cloned().collect();
        format!("TOMLPlusDocument(sections={:?})", sections)
    }

    #[pyo3(signature = (key, default=None))]
    fn get<'py>(
        &self,
        py: Python<'py>,
        key: &str,
        default: Option<PyObject>,
    ) -> PyObject {
        match self.doc.config.get(key) {
            Some(v) => value_to_py(py, v),
            None => default.unwrap_or_else(|| py.None()),
        }
    }

    fn keys<'py>(&self, py: Python<'py>) -> PyObject {
        let xs: Vec<&String> = self.doc.config.keys().collect();
        xs.into_py(py)
    }

    fn values<'py>(&self, py: Python<'py>) -> PyObject {
        let xs: Vec<PyObject> = self.doc.config.values().map(|v| value_to_py(py, v)).collect();
        xs.into_py(py)
    }

    fn items<'py>(&self, py: Python<'py>) -> PyObject {
        let xs: Vec<(String, PyObject)> = self
            .doc
            .config
            .iter()
            .map(|(k, v)| (k.clone(), value_to_py(py, v)))
            .collect();
        xs.into_py(py)
    }

    #[getter]
    fn config<'py>(&self, py: Python<'py>) -> PyObject {
        dict_to_py(py, &self.doc.config)
    }

    #[getter]
    fn meta<'py>(&self, py: Python<'py>) -> PyObject {
        let d = PyDict::new_bound(py);
        for (k, anns) in &self.doc.meta {
            let list = PyList::empty_bound(py);
            for a in anns {
                let pa = Py::new(py, PyAnnotation { inner: a.clone() }).unwrap();
                list.append(pa).ok();
            }
            d.set_item(k, list).ok();
        }
        d.into_py(py)
    }

    #[getter]
    fn vars<'py>(&self, py: Python<'py>) -> PyObject {
        let d = PyDict::new_bound(py);
        for (k, v) in &self.doc.vars {
            d.set_item(k, value_to_py(py, v)).ok();
        }
        d.into_py(py)
    }

    #[pyo3(signature = (key_path, default=None))]
    fn resolve<'py>(
        &self,
        py: Python<'py>,
        key_path: &str,
        default: Option<PyObject>,
    ) -> PyObject {
        match resolve_dotted(&self.doc.config, key_path) {
            Some(v) => value_to_py(py, v),
            None => default.unwrap_or_else(|| py.None()),
        }
    }

    fn annotations(&self, key_path: &str) -> Vec<PyAnnotation> {
        self.doc
            .meta
            .get(key_path)
            .map(|v| v.iter().cloned().map(|a| PyAnnotation { inner: a }).collect())
            .unwrap_or_default()
    }

    fn has_annotation(&self, key_path: &str, name: &str) -> bool {
        self.doc
            .meta
            .get(key_path)
            .map(|v| v.iter().any(|a| a.name == name))
            .unwrap_or(false)
    }

    fn tags<'py>(&self, py: Python<'py>, key_path: &str) -> PyObject {
        let d = PyDict::new_bound(py);
        if let Some(anns) = self.doc.meta.get(key_path) {
            for a in anns {
                if a.name == "tag" {
                    if let AnnotationArg::String(s) = &a.arg {
                        if let Some((k, v)) = s.split_once('=') {
                            let key = k.trim();
                            let val = v.trim().trim_matches('"');
                            d.set_item(key, val).ok();
                        }
                    }
                }
            }
        }
        d.into_py(py)
    }

    fn required_keys(&self) -> Vec<String> {
        self.doc
            .meta
            .iter()
            .filter(|(_, anns)| anns.iter().any(|a| a.name == "required"))
            .map(|(k, _)| k.clone())
            .collect()
    }

    fn deprecated_keys<'py>(&self, py: Python<'py>) -> PyObject {
        let mut out: Vec<(String, Option<String>)> = Vec::new();
        for (k, anns) in &self.doc.meta {
            for a in anns {
                if a.name == "deprecated" {
                    let msg = match &a.arg {
                        AnnotationArg::String(s) => Some(s.clone()),
                        _ => None,
                    };
                    out.push((k.clone(), msg));
                }
            }
        }
        out.into_py(py)
    }

    fn keys_with_tag<'py>(&self, py: Python<'py>, tag_name: &str) -> PyObject {
        let mut out: Vec<(String, String)> = Vec::new();
        for (k, anns) in &self.doc.meta {
            for a in anns {
                if a.name == "tag" {
                    if let AnnotationArg::String(s) = &a.arg {
                        if let Some((tk, tv)) = s.split_once('=') {
                            if tk.trim() == tag_name {
                                out.push((k.clone(), tv.trim().trim_matches('"').to_string()));
                            }
                        }
                    }
                }
            }
        }
        out.into_py(py)
    }
}

#[pyclass]
struct KeyIter {
    keys: Vec<String>,
    idx: usize,
}

#[pymethods]
impl KeyIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<String> {
        if slf.idx >= slf.keys.len() {
            return None;
        }
        let k = slf.keys[slf.idx].clone();
        slf.idx += 1;
        Some(k)
    }
}

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

// ── Value <-> Python conversion ──────────────────────────────────────────────

fn value_to_py(py: Python<'_>, v: &Value) -> PyObject {
    match v {
        Value::Null       => py.None(),
        Value::Bool(b)    => b.into_py(py),
        Value::Integer(n) => n.into_py(py),
        Value::Float(f)   => f.into_py(py),
        Value::String(s)  => s.into_py(py),
        Value::Array(xs)  => {
            let list = PyList::empty_bound(py);
            for x in xs {
                list.append(value_to_py(py, x)).ok();
            }
            list.into_py(py)
        }
        Value::Dict(d) => dict_to_py(py, d),
    }
}

fn dict_to_py(py: Python<'_>, d: &BTreeMap<String, Value>) -> PyObject {
    let pd = PyDict::new_bound(py);
    for (k, v) in d {
        pd.set_item(k, value_to_py(py, v)).ok();
    }
    pd.into_py(py)
}

fn py_to_value(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    if obj.is_none() {
        return Ok(Value::Null);
    }
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(Value::Bool(b));
    }
    if let Ok(n) = obj.extract::<i64>() {
        return Ok(Value::Integer(n));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(Value::Float(f));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(Value::String(s));
    }
    if let Ok(d) = obj.downcast::<PyDict>() {
        let mut map = BTreeMap::new();
        for (k, v) in d.iter() {
            let key: String = k.extract()?;
            map.insert(key, py_to_value(py, &v)?);
        }
        return Ok(Value::Dict(map));
    }
    if let Ok(xs) = obj.downcast::<PyList>() {
        let mut out = Vec::with_capacity(xs.len());
        for item in xs.iter() {
            out.push(py_to_value(py, &item)?);
        }
        return Ok(Value::Array(out));
    }
    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "unsupported value type for dumps: {}",
        obj.get_type().qualname()?,
    )))
}

// ── Top-level functions ──────────────────────────────────────────────────────

#[pyfunction]
fn parse(source: &str) -> PyResult<PyDocument> {
    let doc = parser::parse(source);
    let idx = LineIndex::new(source);
    if let Some(first) = doc
        .diagnostics
        .iter()
        .find(|d| matches!(d.severity, Severity::Error))
    {
        let (line, col) = idx.position(first.span.start);
        let msg = format!("{} (line {}, col {})", first.message, line + 1, col + 1);
        let err = match first.code {
            DiagnosticCode::UndefinedVariable => VariableError::new_err(msg),
            _ => ParseError::new_err(msg),
        };
        return Err(err);
    }
    Ok(PyDocument { doc })
}

#[pyfunction]
fn validate(doc: &PyDocument) -> PyResult<()> {
    let errs = validator::validate(&doc.doc);
    if let Some(first) = errs.iter().find(|d| matches!(d.severity, Severity::Error)) {
        return Err(ValidationError::new_err(first.message.clone()));
    }
    Ok(())
}

#[pyfunction]
fn validate_all<'py>(py: Python<'py>, doc: &PyDocument) -> PyObject {
    let errs = validator::validate(&doc.doc);
    let out: Vec<PyObject> = errs
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .map(|d| {
            let exc = ValidationError::new_err(d.message.clone());
            exc.into_py(py)
        })
        .collect();
    out.into_py(py)
}

#[pyfunction]
fn dumps(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<String> {
    // Accept a TOMLPlusDocument (re-dump with metadata) OR a plain dict.
    if let Ok(doc) = data.extract::<PyRef<PyDocument>>() {
        return Ok(dumper::dumps(&doc.doc));
    }
    if let Ok(d) = data.downcast::<PyDict>() {
        let v = py_to_value(py, d.as_any())?;
        let mut shim = parser::Document::default();
        if let Value::Dict(m) = v {
            shim.config = m;
        }
        return Ok(dumper::dumps(&shim));
    }
    Err(pyo3::exceptions::PyTypeError::new_err(
        "dumps expects a TOMLPlusDocument or a dict",
    ))
}

// ── Module init ──────────────────────────────────────────────────────────────

#[pymodule]
#[pyo3(name = "_native")]
fn _native(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyDocument>()?;
    m.add_class::<PyAnnotation>()?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(validate, m)?)?;
    m.add_function(wrap_pyfunction!(validate_all, m)?)?;
    m.add_function(wrap_pyfunction!(dumps, m)?)?;
    m.add("TOMLPlusError", py.get_type_bound::<TOMLPlusError>())?;
    m.add("ParseError", py.get_type_bound::<ParseError>())?;
    m.add("ValidationError", py.get_type_bound::<ValidationError>())?;
    m.add("VariableError", py.get_type_bound::<VariableError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    // Silence dead-code on conversion helpers if no callers reach them in tests.
    let _ = (PyString::new_bound, PyTuple::empty_bound);
    Ok(())
}
