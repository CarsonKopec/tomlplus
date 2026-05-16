//! Diagnostic types — used by both the parser and validator.

use crate::span::Span;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
    pub severity: Severity,
    pub code: DiagnosticCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    /// Generic syntax error.
    Syntax,
    /// Reference to an undefined `$variable` or `[vars]` entry.
    UndefinedVariable,
    /// Annotation-line itself is malformed.
    BadAnnotation,
    /// A constraint annotation failed against its value.
    Validation,
    /// Use of a `@deprecated` key — surfaced as a warning.
    Deprecated,
    /// Duplicate key/section name.
    Duplicate,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Span, code: DiagnosticCode) -> Self {
        Self { message: message.into(), span, severity: Severity::Error, code }
    }

    pub fn warning(message: impl Into<String>, span: Span, code: DiagnosticCode) -> Self {
        Self { message: message.into(), span, severity: Severity::Warning, code }
    }
}

#[derive(Debug, Error, Clone)]
pub enum ParseError {
    #[error("{message}")]
    Generic { message: String, span: Span },
}

impl ParseError {
    pub fn span(&self) -> Span {
        match self {
            ParseError::Generic { span, .. } => *span,
        }
    }

    pub fn into_diagnostic(self, code: DiagnosticCode) -> Diagnostic {
        match self {
            ParseError::Generic { message, span } => Diagnostic::error(message, span, code),
        }
    }
}
