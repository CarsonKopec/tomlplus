//! TOML+ language core.
//!
//! Layered modules:
//!   * [`span`]          — byte spans + UTF-8 line indexing
//!   * [`error`]         — diagnostics + parse errors
//!   * [`value`]         — runtime values
//!   * [`lexer`]         — logical-line tokenizer
//!   * [`value_parser`]  — RHS expression parser (numbers, strings, vars, etc.)
//!   * [`annotation`]    — `@annotation` parser
//!   * [`parser`]        — full document parser ([`parser::Document`])
//!   * [`validator`]     — annotation-driven validator
//!   * [`dumper`]        — round-trip serializer
//!
//! Convenience top-level entry points re-export the most-used pieces.

pub mod annotation;
pub mod dumper;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod validator;
pub mod value;
pub mod value_parser;

pub use annotation::{Annotation, AnnotationArg};
pub use error::{Diagnostic, DiagnosticCode, ParseError, Severity};
pub use parser::{parse, parse_with_env, Document};
pub use span::{LineIndex, Span};
pub use validator::validate;
pub use value::Value;

/// Builtin variable names that are always available in TOML+ source.
/// The LSP uses this for completion.
pub const BUILTIN_VARS: &[&str] = &[
    "NOW", "TODAY", "TRUE", "FALSE", "NULL",
    "PID", "HOSTNAME", "PLATFORM", "CWD",
];

/// Annotation names known to the validator. Used by the LSP for completion.
pub const KNOWN_ANNOTATIONS: &[&str] = &[
    "required", "type", "min", "max", "minlen", "maxlen", "pattern",
    "enum", "positive", "nonzero", "nonempty", "deprecated",
    "tag", "internal", "readonly", "experimental",
];

/// Annotation type names recognised by `@type:` for completion.
pub const KNOWN_TYPES: &[&str] = &[
    "string", "int", "float", "bool", "dict", "list",
    "list[string]", "list[int]", "list[float]", "list[bool]",
    "url", "email", "path", "duration",
];
