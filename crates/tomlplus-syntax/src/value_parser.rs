//! Parses a value-expression string (the right-hand side of `key = …`) into
//! a [`Value`].
//!
//! Layered grammar:
//!     expr   ::= arith ( "??" expr )?
//!     arith  ::= atom (("+"|"-"|"*"|"/") atom)*
//!     atom   ::= variable | string | bool | null | number | array | dict
//!     array  ::= "["   ( expr ("," expr)* ","? )?  "]"
//!     dict   ::= "#{"  ( entry ("," entry)* ","? )? "}#"
//!     entry  ::= (bare_key | quoted_key) "=" expr

use std::collections::BTreeMap;

use crate::error::{ParseError, Severity};
use crate::span::Span;
use crate::value::Value;

pub struct ValueParser<'a> {
    /// Merged user vars + builtins.
    pub vars: &'a BTreeMap<String, Value>,
    /// Read-back of `$ENV.X` references (None if env var is unset).
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// Source span the entire RHS lives at — added to local offsets to make
    /// diagnostics absolute.
    pub origin: Span,
    /// Records every `$variable` reference we resolve (for goto-def, hover).
    pub var_refs: Vec<VarRef>,
}

#[derive(Debug, Clone)]
pub struct VarRef {
    pub name: String,
    pub kind: VarRefKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarRefKind {
    User,
    Builtin,
    Env,
}

impl<'a> ValueParser<'a> {
    pub fn new(
        vars: &'a BTreeMap<String, Value>,
        env: &'a dyn Fn(&str) -> Option<String>,
        origin: Span,
    ) -> Self {
        Self { vars, env, origin, var_refs: Vec::new() }
    }

    pub fn parse(&mut self, input: &str) -> Result<Value, ParseError> {
        self.expr(input.trim(), self.origin.start + leading_ws(input))
    }

    fn span(&self, local_start: usize, len: usize) -> Span {
        Span::new(local_start, local_start + len)
    }

    // ── expr (??) ──────────────────────────────────────────────────────────

    fn expr(&mut self, raw: &str, abs_start: usize) -> Result<Value, ParseError> {
        if let Some(idx) = find_op_top_level(raw, "??") {
            let lhs_raw = raw[..idx].trim();
            let rhs_raw = raw[idx + 2..].trim();

            let lhs_offset = abs_start + leading_ws(&raw[..idx]);
            let rhs_offset = abs_start + idx + 2 + leading_ws(&raw[idx + 2..]);

            let lhs = self.arith(lhs_raw, lhs_offset)?;
            if !matches!(lhs, Value::Null) {
                return Ok(lhs);
            }
            return self.expr(rhs_raw, rhs_offset);
        }
        self.arith(raw, abs_start)
    }

    // ── arith (+ - * /) ────────────────────────────────────────────────────

    fn arith(&mut self, raw: &str, abs_start: usize) -> Result<Value, ParseError> {
        let toks = tokenize_arith(raw, abs_start);
        if toks.is_empty() {
            return Err(ParseError::Generic {
                message: "empty value".into(),
                span: Span::new(abs_start, abs_start),
            });
        }
        if toks.len() == 1 {
            return self.atom(&toks[0].text, toks[0].abs_start);
        }
        // Left-associative fold.
        let mut result = self.atom(&toks[0].text, toks[0].abs_start)?;
        let mut i = 1;
        while i + 1 < toks.len() {
            let op = toks[i].text.chars().next().unwrap();
            let rhs = self.atom(&toks[i + 1].text, toks[i + 1].abs_start)?;
            result = apply_binop(op, result, rhs, toks[i].abs_start)?;
            i += 2;
        }
        Ok(result)
    }

    // ── atom ───────────────────────────────────────────────────────────────

    fn atom(&mut self, raw: &str, abs_start: usize) -> Result<Value, ParseError> {
        let raw = raw.trim();
        let local_start = abs_start + leading_ws_str(raw);
        let _ = local_start;

        if raw.starts_with('$') {
            return self.variable(raw, abs_start);
        }
        if raw.starts_with('"') {
            return parse_string(raw, self.span(abs_start, raw.len()));
        }
        if raw == "true" {
            return Ok(Value::Bool(true));
        }
        if raw == "false" {
            return Ok(Value::Bool(false));
        }
        if raw == "null" {
            return Ok(Value::Null);
        }
        if raw.starts_with("#{") {
            return self.inline_dict(raw, abs_start);
        }
        if raw.starts_with('[') {
            return self.array(raw, abs_start);
        }
        if let Some(n) = parse_number(raw) {
            return Ok(n);
        }
        Err(ParseError::Generic {
            message: format!("cannot parse value: `{}`", raw),
            span: self.span(abs_start, raw.len()),
        })
    }

    fn variable(&mut self, raw: &str, abs_start: usize) -> Result<Value, ParseError> {
        let name = &raw[1..]; // strip $

        if let Some(env_key) = name.strip_prefix("ENV.") {
            self.var_refs.push(VarRef {
                name: env_key.to_string(),
                kind: VarRefKind::Env,
                span: self.span(abs_start, raw.len()),
            });
            return Ok(match (self.env)(env_key) {
                Some(v) => Value::String(v),
                None => Value::Null,
            });
        }

        if let Some(v) = builtin_value(name) {
            self.var_refs.push(VarRef {
                name: name.to_string(),
                kind: VarRefKind::Builtin,
                span: self.span(abs_start, raw.len()),
            });
            return Ok(v);
        }

        if let Some(v) = self.vars.get(name) {
            self.var_refs.push(VarRef {
                name: name.to_string(),
                kind: VarRefKind::User,
                span: self.span(abs_start, raw.len()),
            });
            return Ok(v.clone());
        }

        Err(ParseError::Generic {
            message: format!("undefined variable `${}`", name),
            span: self.span(abs_start, raw.len()),
        })
    }

    fn array(&mut self, raw: &str, abs_start: usize) -> Result<Value, ParseError> {
        if !(raw.starts_with('[') && raw.ends_with(']')) {
            return Err(ParseError::Generic {
                message: "malformed array".into(),
                span: self.span(abs_start, raw.len()),
            });
        }
        let inner = &raw[1..raw.len() - 1];
        let inner_start = abs_start + 1;
        let mut items = Vec::new();
        for (part, off) in split_top_level(inner, ',', inner_start) {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            let part_off = off + (part.len() - part.trim_start().len());
            items.push(self.expr(trimmed, part_off)?);
        }
        Ok(Value::Array(items))
    }

    fn inline_dict(&mut self, raw: &str, abs_start: usize) -> Result<Value, ParseError> {
        if !(raw.starts_with("#{") && raw.ends_with("}#")) {
            return Err(ParseError::Generic {
                message: "malformed inline dict".into(),
                span: self.span(abs_start, raw.len()),
            });
        }
        let inner = &raw[2..raw.len() - 2];
        let inner_start = abs_start + 2;
        let mut map = BTreeMap::new();
        for (part, off) in split_top_level(inner, ',', inner_start) {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            let eq_idx = match find_op_top_level(trimmed, "=") {
                Some(i) => i,
                None => {
                    return Err(ParseError::Generic {
                        message: format!("inline dict entry missing `=`: `{}`", trimmed),
                        span: self.span(off, trimmed.len()),
                    })
                }
            };
            let key_raw = trimmed[..eq_idx].trim();
            let val_raw = trimmed[eq_idx + 1..].trim();
            let key = if key_raw.starts_with('"') && key_raw.ends_with('"') && key_raw.len() >= 2 {
                key_raw[1..key_raw.len() - 1].to_string()
            } else {
                key_raw.to_string()
            };
            let val_off = off + (part.len() - part.trim_start().len()) + eq_idx + 1;
            let val_off = val_off + leading_ws(&trimmed[eq_idx + 1..]);
            let v = self.expr(val_raw, val_off)?;
            map.insert(key, v);
        }
        Ok(Value::Dict(map))
    }
}

fn leading_ws(s: &str) -> usize {
    s.len() - s.trim_start().len()
}

fn leading_ws_str(s: &str) -> usize {
    s.len() - s.trim_start().len()
}

fn apply_binop(op: char, lhs: Value, rhs: Value, abs_start: usize) -> Result<Value, ParseError> {
    match op {
        '+' => match (&lhs, &rhs) {
            (Value::String(_), _) | (_, Value::String(_)) => {
                let mut s = stringify(&lhs);
                s.push_str(&stringify(&rhs));
                Ok(Value::String(s))
            }
            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a + b)),
            (Value::Float(a), Value::Float(b))     => Ok(Value::Float(a + b)),
            (Value::Integer(a), Value::Float(b))   => Ok(Value::Float(*a as f64 + b)),
            (Value::Float(a), Value::Integer(b))   => Ok(Value::Float(a + *b as f64)),
            _ => binop_err("+", abs_start),
        },
        '-' => match (&lhs, &rhs) {
            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a - b)),
            (Value::Float(a), Value::Float(b))     => Ok(Value::Float(a - b)),
            (Value::Integer(a), Value::Float(b))   => Ok(Value::Float(*a as f64 - b)),
            (Value::Float(a), Value::Integer(b))   => Ok(Value::Float(a - *b as f64)),
            _ => binop_err("-", abs_start),
        },
        '*' => match (&lhs, &rhs) {
            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a * b)),
            (Value::Float(a), Value::Float(b))     => Ok(Value::Float(a * b)),
            (Value::Integer(a), Value::Float(b))   => Ok(Value::Float(*a as f64 * b)),
            (Value::Float(a), Value::Integer(b))   => Ok(Value::Float(a * *b as f64)),
            _ => binop_err("*", abs_start),
        },
        '/' => match (&lhs, &rhs) {
            (Value::Integer(a), Value::Integer(b)) if *b != 0 => Ok(Value::Float(*a as f64 / *b as f64)),
            (Value::Float(a), Value::Float(b))     => Ok(Value::Float(a / b)),
            (Value::Integer(a), Value::Float(b))   => Ok(Value::Float(*a as f64 / b)),
            (Value::Float(a), Value::Integer(b)) if *b != 0 => Ok(Value::Float(a / *b as f64)),
            _ => binop_err("/", abs_start),
        },
        _ => binop_err(&op.to_string(), abs_start),
    }
}

fn binop_err(op: &str, abs_start: usize) -> Result<Value, ParseError> {
    Err(ParseError::Generic {
        message: format!("invalid operands for `{}`", op),
        span: Span::new(abs_start, abs_start + op.len()),
    })
}

fn stringify(v: &Value) -> String {
    match v {
        Value::String(s)  => s.clone(),
        Value::Integer(n) => n.to_string(),
        Value::Float(f)   => f.to_string(),
        Value::Bool(b)    => b.to_string(),
        Value::Null       => "null".to_string(),
        Value::Array(_) | Value::Dict(_) => format!("{:?}", v),
    }
}

fn parse_string(raw: &str, span: Span) -> Result<Value, ParseError> {
    if !(raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2) {
        return Err(ParseError::Generic {
            message: "unterminated string".into(),
            span,
        });
    }
    let inner = &raw[1..raw.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n')  => out.push('\n'),
                Some('t')  => out.push('\t'),
                Some('r')  => out.push('\r'),
                Some('"')  => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    Ok(Value::String(out))
}

fn parse_number(raw: &str) -> Option<Value> {
    let (sign, rest) = match raw.strip_prefix('-') {
        Some(r) => (-1i64, r),
        None    => (1, raw),
    };
    let cleaned: String = rest.chars().filter(|c| *c != '_').collect();

    // Hex / oct / bin
    if let Some(hex) = cleaned.strip_prefix("0x").or_else(|| cleaned.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).ok().map(|n| Value::Integer(sign * n));
    }
    if let Some(oct) = cleaned.strip_prefix("0o").or_else(|| cleaned.strip_prefix("0O")) {
        return i64::from_str_radix(oct, 8).ok().map(|n| Value::Integer(sign * n));
    }
    if let Some(bin) = cleaned.strip_prefix("0b").or_else(|| cleaned.strip_prefix("0B")) {
        return i64::from_str_radix(bin, 2).ok().map(|n| Value::Integer(sign * n));
    }

    if cleaned.contains('.') || cleaned.contains('e') || cleaned.contains('E') {
        cleaned.parse::<f64>().ok().map(|f| Value::Float(sign as f64 * f))
    } else {
        cleaned.parse::<i64>().ok().map(|n| Value::Integer(sign * n))
    }
}

fn builtin_value(name: &str) -> Option<Value> {
    Some(match name {
        "TRUE"  => Value::Bool(true),
        "FALSE" => Value::Bool(false),
        "NULL"  => Value::Null,
        "NOW"   => Value::String(chrono::Utc::now().to_rfc3339()),
        "TODAY" => Value::String(chrono::Utc::now().format("%Y-%m-%d").to_string()),
        "PLATFORM" => Value::String(std::env::consts::OS.to_string()),
        "PID"      => process_id(),
        "HOSTNAME" => host_name(),
        "CWD"      => current_working_dir(),
        _ => return None,
    })
}

// OS-specific builtins. On wasm32 there's no process/host/filesystem to
// query, so we degrade gracefully to empty strings / zero.

#[cfg(not(target_family = "wasm"))]
fn process_id() -> Value {
    Value::Integer(std::process::id() as i64)
}
#[cfg(target_family = "wasm")]
fn process_id() -> Value {
    Value::Integer(0)
}

#[cfg(not(target_family = "wasm"))]
fn host_name() -> Value {
    Value::String(
        hostname::get()
            .ok()
            .and_then(|s| s.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string()),
    )
}
#[cfg(target_family = "wasm")]
fn host_name() -> Value {
    Value::String(String::new())
}

#[cfg(not(target_family = "wasm"))]
fn current_working_dir() -> Value {
    Value::String(
        std::env::current_dir()
            .ok()
            .and_then(|p| p.to_str().map(str::to_string))
            .unwrap_or_default(),
    )
}
#[cfg(target_family = "wasm")]
fn current_working_dir() -> Value {
    Value::String(String::new())
}

/// `_` is the level-0 separator for `,` etc. Returns `(slice, abs_start_of_slice)`.
fn split_top_level(s: &str, sep: char, abs_start: usize) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut buf_start = abs_start;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;

    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if esc {
            esc = false;
            buf.push(b as char);
            i += 1;
            continue;
        }
        if b == b'\\' {
            esc = true;
            buf.push(b as char);
            i += 1;
            continue;
        }
        if b == b'"' {
            in_str = !in_str;
            buf.push(b as char);
            i += 1;
            continue;
        }
        if !in_str {
            if b == b'#' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                depth += 1;
                buf.push('#');
                buf.push('{');
                i += 2;
                continue;
            }
            if b == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'#' {
                depth -= 1;
                buf.push('}');
                buf.push('#');
                i += 2;
                continue;
            }
            match b {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                _ => {}
            }
            if depth == 0 && b as char == sep {
                out.push((buf.clone(), buf_start));
                buf.clear();
                buf_start = abs_start + i + 1;
                i += 1;
                continue;
            }
        }
        buf.push(b as char);
        i += 1;
    }
    if !buf.is_empty() {
        out.push((buf, buf_start));
    }
    out
}

fn find_op_top_level(s: &str, op: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let op_bytes = op.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if esc {
            esc = false;
            i += 1;
            continue;
        }
        if b == b'\\' {
            esc = true;
            i += 1;
            continue;
        }
        if b == b'"' {
            in_str = !in_str;
            i += 1;
            continue;
        }
        if !in_str {
            if b == b'#' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                depth += 1;
                i += 2;
                continue;
            }
            if b == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'#' {
                depth -= 1;
                i += 2;
                continue;
            }
            match b {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                _ => {}
            }
            if depth == 0 && bytes[i..].starts_with(op_bytes) {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

#[derive(Debug)]
struct ArithToken {
    text: String,
    abs_start: usize,
}

fn tokenize_arith(raw: &str, abs_start: usize) -> Vec<ArithToken> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut buf_start = abs_start;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut prev_was_op_or_start = true;

    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            in_str = !in_str;
            buf.push('"');
            i += 1;
            continue;
        }
        if !in_str {
            if b == b'#' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                depth += 1;
                buf.push_str("#{");
                i += 2;
                continue;
            }
            if b == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'#' {
                depth -= 1;
                buf.push_str("}#");
                i += 2;
                continue;
            }
            match b {
                b'(' | b'[' | b'{' => {
                    depth += 1;
                    buf.push(b as char);
                    i += 1;
                    continue;
                }
                b')' | b']' | b'}' => {
                    depth -= 1;
                    buf.push(b as char);
                    i += 1;
                    continue;
                }
                _ => {}
            }
            let is_op = matches!(b, b'+' | b'-' | b'*' | b'/');
            // `//` is not an operator (treat as part of value, e.g. paths)
            if is_op
                && depth == 0
                && !(b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/')
            {
                let unary_minus = b == b'-' && (buf.trim().is_empty() || prev_was_op_or_start);
                if unary_minus {
                    if buf.trim().is_empty() {
                        buf_start = abs_start + i;
                    }
                    buf.push('-');
                    i += 1;
                    prev_was_op_or_start = false;
                    continue;
                }
                let trimmed = buf.trim().to_string();
                if !trimmed.is_empty() {
                    out.push(ArithToken { text: trimmed, abs_start: buf_start });
                }
                out.push(ArithToken { text: (b as char).to_string(), abs_start: abs_start + i });
                buf.clear();
                buf_start = abs_start + i + 1;
                prev_was_op_or_start = true;
                i += 1;
                continue;
            }
        }
        if buf.is_empty() && !b.is_ascii_whitespace() {
            buf_start = abs_start + i;
        }
        buf.push(b as char);
        prev_was_op_or_start = false;
        i += 1;
    }
    let trimmed = buf.trim().to_string();
    if !trimmed.is_empty() {
        out.push(ArithToken { text: trimmed, abs_start: buf_start });
    }
    let _ = Severity::Error; // silence unused-import for now
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_env(_: &str) -> Option<String> { None }

    fn parse_value(s: &str) -> Value {
        let vars = BTreeMap::new();
        let mut vp = ValueParser::new(&vars, &empty_env, Span::new(0, s.len()));
        vp.parse(s).unwrap()
    }

    #[test]
    fn integers() {
        assert_eq!(parse_value("42"), Value::Integer(42));
        assert_eq!(parse_value("-7"), Value::Integer(-7));
        assert_eq!(parse_value("0xff"), Value::Integer(255));
        assert_eq!(parse_value("0o755"), Value::Integer(0o755));
        assert_eq!(parse_value("0b1010"), Value::Integer(0b1010));
        assert_eq!(parse_value("1_000"), Value::Integer(1000));
    }

    #[test]
    fn floats() {
        assert_eq!(parse_value("3.14"), Value::Float(3.14));
        assert_eq!(parse_value("1.5e3"), Value::Float(1500.0));
    }

    #[test]
    fn bools_null() {
        assert_eq!(parse_value("true"), Value::Bool(true));
        assert_eq!(parse_value("false"), Value::Bool(false));
        assert_eq!(parse_value("null"), Value::Null);
    }

    #[test]
    fn string_escapes() {
        assert_eq!(parse_value("\"a\\nb\""), Value::String("a\nb".to_string()));
    }

    #[test]
    fn array_with_exprs() {
        let v = parse_value("[1, 2, 3]");
        assert_eq!(
            v,
            Value::Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ])
        );
    }

    #[test]
    fn inline_dict() {
        let v = parse_value("#{ a = 1, b = 2 }#");
        let Value::Dict(d) = v else { panic!("not a dict") };
        assert_eq!(d["a"], Value::Integer(1));
        assert_eq!(d["b"], Value::Integer(2));
    }

    #[test]
    fn fallback_picks_rhs_on_null() {
        assert_eq!(parse_value("null ?? 99"), Value::Integer(99));
        assert_eq!(parse_value("\"x\" ?? 99"), Value::String("x".into()));
    }

    #[test]
    fn arithmetic_left_assoc_no_precedence() {
        // Mirror Python: left-associative, no operator precedence.
        // 2 + 3 * 4 → (2+3)*4 = 20.
        assert_eq!(parse_value("2 + 3 * 4"), Value::Integer(20));
    }

    #[test]
    fn string_concat() {
        assert_eq!(
            parse_value("\"hello\" + \" \" + \"world\""),
            Value::String("hello world".to_string())
        );
    }
}
