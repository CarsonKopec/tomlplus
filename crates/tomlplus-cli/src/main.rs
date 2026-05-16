//! `tomlpr` — small command-line front-end to the TOML+ language core.
//!
//!     tomlpr parse    <file>          parse + print as JSON
//!     tomlpr validate <file>          parse + run annotation validators
//!     tomlpr fmt      <file>          round-trip through the dumper
//!     tomlpr vars     <file>          print resolved [vars] (+ builtins)

use std::process::ExitCode;

use tomlplus_syntax::{
    dumper, parser, validator,
    value::Value,
    Severity, LineIndex, BUILTIN_VARS,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, path) = match args.as_slice() {
        [c, p] => (c.as_str(), p.as_str()),
        _ => {
            eprintln!("usage: tomlpr <parse|validate|fmt|vars> <file.tomlp>");
            return ExitCode::from(2);
        }
    };

    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {}: {}", path, e);
            return ExitCode::FAILURE;
        }
    };

    let doc = parser::parse(&text);
    let line_index = LineIndex::new(&text);

    // Surface parse-time diagnostics no matter the subcommand.
    let mut had_error = false;
    for d in &doc.diagnostics {
        let (line, col) = line_index.position(d.span.start);
        let kind = match d.severity {
            Severity::Error   => { had_error = true; "error" }
            Severity::Warning => "warning",
            Severity::Info    => "info",
            Severity::Hint    => "hint",
        };
        eprintln!("{}:{}:{}: {}: {}", path, line + 1, col + 1, kind, d.message);
    }

    match cmd {
        "parse" => {
            let json = value_to_json(&Value::Dict(doc.config.clone()));
            match serde_json::to_string_pretty(&json) {
                Ok(s) => println!("{}", s),
                Err(e) => {
                    eprintln!("error: {}", e);
                    return ExitCode::FAILURE;
                }
            }
        }
        "validate" => {
            let errs = validator::validate(&doc);
            if errs.is_empty() && !had_error {
                println!("✓ {} annotated keys validated", doc.meta.len());
            } else {
                for d in &errs {
                    let (line, col) = line_index.position(d.span.start);
                    let kind = if matches!(d.severity, Severity::Warning) { "warning" } else { "error" };
                    eprintln!("{}:{}:{}: {}: {}", path, line + 1, col + 1, kind, d.message);
                }
                let fatal = errs.iter().any(|d| matches!(d.severity, Severity::Error));
                if fatal || had_error {
                    return ExitCode::FAILURE;
                }
            }
        }
        "fmt" => {
            print!("{}", dumper::dumps(&doc));
        }
        "vars" => {
            if !doc.vars.is_empty() {
                println!("User-defined:");
                for (k, v) in &doc.vars {
                    println!("  ${:<24} = {}", k, format_value(v));
                }
                println!();
            }
            println!("Built-ins:");
            for b in BUILTIN_VARS {
                let resolved = match tomlplus_syntax::parser::parse(&format!("_x = ${}", b))
                    .config
                    .get("_x")
                    .cloned()
                {
                    Some(v) => format_value(&v),
                    None => "<unresolved>".to_string(),
                };
                println!("  ${:<24} = {}", b, resolved);
            }
        }
        other => {
            eprintln!("unknown command: {}", other);
            return ExitCode::from(2);
        }
    }

    if had_error { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

fn format_value(v: &Value) -> String {
    match v {
        Value::String(s)  => format!("{:?}", s),
        Value::Integer(n) => n.to_string(),
        Value::Float(f)   => f.to_string(),
        Value::Bool(b)    => b.to_string(),
        Value::Null       => "null".into(),
        Value::Array(_) | Value::Dict(_) => serde_json::to_string(&value_to_json(v)).unwrap(),
    }
}

fn value_to_json(v: &Value) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        Value::Null       => J::Null,
        Value::Bool(b)    => J::Bool(*b),
        Value::Integer(n) => J::Number((*n).into()),
        Value::Float(f)   => serde_json::Number::from_f64(*f).map(J::Number).unwrap_or(J::Null),
        Value::String(s)  => J::String(s.clone()),
        Value::Array(xs)  => J::Array(xs.iter().map(value_to_json).collect()),
        Value::Dict(d)    => J::Object(
            d.iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect(),
        ),
    }
}
