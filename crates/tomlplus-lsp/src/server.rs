//! LSP backend.
//!
//! Reparses on every change (cheap for config files) and republishes
//! diagnostics + semantic tokens. Per-document state lives in a `DashMap`
//! keyed by URI.

use dashmap::DashMap;
use ropey::Rope;
use std::sync::Arc;

use tomlplus_syntax::{
    annotation::{Annotation, AnnotationArg},
    dumper, parser, validator,
    value::Value as TpValue,
    value_parser::VarRefKind,
    Diagnostic as TpDiagnostic, LineIndex, Severity, Span,
    BUILTIN_VARS, KNOWN_ANNOTATIONS, KNOWN_TYPES,
};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

// ── Semantic-token legend ────────────────────────────────────────────────────
// These indices MUST match the order in the legend we declare in `initialize`.
mod tok {
    pub const NAMESPACE: u32   = 0;  // [section]
    pub const PROPERTY: u32    = 1;  // key
    pub const DECORATOR: u32   = 2;  // @annotation name
    pub const TYPE: u32        = 3;  // @type: <name>
    pub const ENUM_MEMBER: u32 = 4;  // @enum: [a, b, c]
    pub const VARIABLE: u32    = 5;  // $userVar
    pub const _CONSTANT: u32   = 6;  // $BUILTIN
    pub const _PARAMETER: u32  = 7;  // $ENV.X
    pub const _NUMBER: u32     = 8;  // numeric annotation arg
    pub const _STRING: u32     = 9;  // string annotation arg
    pub const _REGEXP: u32     = 10; // @pattern arg
}

// Modifier bits — match the order in the legend.
mod modi {
    pub const DECLARATION: u32 = 1 << 0;
    pub const READONLY: u32    = 1 << 1;
    pub const STATIC: u32      = 1 << 2;
    pub const DEPRECATED: u32  = 1 << 3;
    pub const DEFAULT_LIB: u32 = 1 << 4;
}

#[derive(Debug)]
pub struct Backend {
    client: Client,
    docs: Arc<DashMap<Url, DocState>>,
}

#[derive(Debug)]
struct DocState {
    rope: Rope,
    line_index: LineIndex,
    parsed: parser::Document,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self { client, docs: Arc::new(DashMap::new()) }
    }

    fn store_and_diagnose(&self, uri: Url, text: String) -> Vec<Diagnostic> {
        let rope = Rope::from_str(&text);
        let line_index = LineIndex::new(&text);
        let parsed = parser::parse(&text);
        let mut diagnostics: Vec<TpDiagnostic> = parsed.diagnostics.clone();
        diagnostics.extend(validator::validate(&parsed));

        let lsp_diags = diagnostics
            .iter()
            .map(|d| to_lsp_diagnostic(d, &line_index))
            .collect();

        self.docs.insert(uri, DocState { rope, line_index, parsed });
        lsp_diags
    }

    fn with_doc<F, R>(&self, uri: &Url, f: F) -> Option<R>
    where
        F: FnOnce(&DocState) -> R,
    {
        self.docs.get(uri).map(|d| f(&*d))
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "tomlplus-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "@".into(), "$".into(), ".".into(), ":".into(),
                    ]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                color_provider: Some(ColorProviderCapability::Simple(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: vec![
                                    SemanticTokenType::NAMESPACE,    // 0
                                    SemanticTokenType::PROPERTY,     // 1
                                    SemanticTokenType::DECORATOR,    // 2
                                    SemanticTokenType::TYPE,         // 3
                                    SemanticTokenType::ENUM_MEMBER,  // 4
                                    SemanticTokenType::VARIABLE,     // 5
                                    SemanticTokenType::new("constant"),   // 6
                                    SemanticTokenType::PARAMETER,    // 7
                                    SemanticTokenType::NUMBER,       // 8
                                    SemanticTokenType::STRING,       // 9
                                    SemanticTokenType::REGEXP,       // 10
                                ],
                                token_modifiers: vec![
                                    SemanticTokenModifier::DECLARATION,
                                    SemanticTokenModifier::READONLY,
                                    SemanticTokenModifier::STATIC,
                                    SemanticTokenModifier::DEPRECATED,
                                    SemanticTokenModifier::DEFAULT_LIBRARY,
                                ],
                            },
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "tomlplus-lsp ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let diags = self.store_and_diagnose(uri.clone(), params.text_document.text);
        self.client.publish_diagnostics(uri, diags, None).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(change) = params.content_changes.into_iter().last() {
            let diags = self.store_and_diagnose(uri.clone(), change.text);
            self.client.publish_diagnostics(uri, diags, None).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.docs.remove(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        Ok(self.with_doc(&uri, |s| hover_at(s, pos)).flatten())
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        Ok(self
            .with_doc(&uri, |s| completion_at(s, pos))
            .map(CompletionResponse::Array))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        Ok(self
            .with_doc(&params.text_document.uri, document_symbols)
            .map(DocumentSymbolResponse::Nested))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        Ok(self
            .with_doc(&uri, |s| definition_at(s, &uri, pos))
            .flatten())
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        Ok(self.with_doc(&params.text_document.uri, |s| {
            let formatted = dumper::dumps(&s.parsed);
            let end_line = s.rope.len_lines() as u32;
            let range = Range::new(Position::new(0, 0), Position::new(end_line, 0));
            vec![TextEdit { range, new_text: formatted }]
        }))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        Ok(self
            .with_doc(&params.text_document.uri, |s| {
                SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: semantic_tokens(s),
                })
            }))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        Ok(self
            .with_doc(&params.text_document.uri, |s| inlay_hints(s, params.range)))
    }

    async fn document_color(
        &self,
        params: DocumentColorParams,
    ) -> Result<Vec<ColorInformation>> {
        Ok(self
            .with_doc(&params.text_document.uri, document_colors)
            .unwrap_or_default())
    }

    async fn color_presentation(
        &self,
        params: ColorPresentationParams,
    ) -> Result<Vec<ColorPresentation>> {
        Ok(vec![ColorPresentation {
            label: format_color_hex(&params.color),
            text_edit: None,
            additional_text_edits: None,
        }])
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn to_lsp_diagnostic(d: &TpDiagnostic, idx: &LineIndex) -> Diagnostic {
    Diagnostic {
        range: span_to_range(d.span, idx),
        severity: Some(match d.severity {
            Severity::Error   => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Info    => DiagnosticSeverity::INFORMATION,
            Severity::Hint    => DiagnosticSeverity::HINT,
        }),
        source: Some("tomlplus".to_string()),
        message: d.message.clone(),
        ..Default::default()
    }
}

fn span_to_range(span: Span, idx: &LineIndex) -> Range {
    let (sl, sc) = idx.position(span.start);
    let (el, ec) = idx.position(span.end);
    Range::new(Position::new(sl, sc), Position::new(el, ec))
}

fn pos_to_offset(pos: Position, idx: &LineIndex) -> usize {
    idx.line_start(pos.line as usize) + pos.character as usize
}

// ── Hover ────────────────────────────────────────────────────────────────────

fn hover_at(state: &DocState, pos: Position) -> Option<Hover> {
    let offset = pos_to_offset(pos, &state.line_index);

    // 1. Variable reference under cursor?
    if let Some(vref) = state.parsed.var_refs.iter().find(|r| r.span.contains(offset)) {
        return Some(hover_for_var(state, vref));
    }

    // 2. Key under cursor?
    for (key, span) in &state.parsed.key_spans {
        if span.contains(offset) {
            return Some(hover_for_key(state, key, *span));
        }
    }

    None
}

fn hover_for_var(state: &DocState, vref: &tomlplus_syntax::value_parser::VarRef) -> Hover {
    let mut md = String::new();
    match vref.kind {
        VarRefKind::User => {
            md.push_str(&format!("`${}` &nbsp;·&nbsp; **user variable**\n", vref.name));
            if let Some(v) = state.parsed.vars.get(&vref.name) {
                md.push_str(&format!("\n*type:* `{}`\n", v.type_name()));
                md.push_str(&format!("\n```tomlplus\n{} = {}\n```", vref.name, format_value_display(v)));
            }
        }
        VarRefKind::Builtin => {
            md.push_str(&format!("`${}` &nbsp;·&nbsp; **built-in**\n", vref.name));
            md.push_str(builtin_doc(&vref.name));
        }
        VarRefKind::Env => {
            md.push_str(&format!("`$ENV.{}` &nbsp;·&nbsp; **environment variable**\n", vref.name));
            match std::env::var(&vref.name) {
                Ok(v)  => md.push_str(&format!("\n*currently set:* `{}`", v)),
                Err(_) => md.push_str("\n*currently:* `<unset>` — use `??` for a fallback"),
            }
        }
    }
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: Some(span_to_range(vref.span, &state.line_index)),
    }
}

fn hover_for_key(state: &DocState, key: &str, span: Span) -> Hover {
    let mut md = format!("**`{}`**", key);
    let value = resolve_value(&state.parsed.config, key);
    if let Some(v) = value {
        md.push_str(&format!(" &nbsp;·&nbsp; *{}*\n", v.type_name()));
        md.push_str(&format!("\n```tomlplus\n= {}\n```\n", format_value_display(v)));
    } else {
        md.push('\n');
    }

    if let Some(anns) = state.parsed.meta.get(key) {
        let required   = anns.iter().any(|a| a.name == "required");
        let deprecated = anns.iter().find(|a| a.name == "deprecated");
        let mut chips: Vec<String> = Vec::new();
        if required {
            chips.push("`@required`".to_string());
        }
        if let Some(d) = deprecated {
            let msg = match &d.arg {
                AnnotationArg::String(s) => format!("⚠ deprecated — {}", s),
                _ => "⚠ deprecated".to_string(),
            };
            chips.push(msg);
        }
        if !chips.is_empty() {
            md.push('\n');
            md.push_str(&chips.join(" &nbsp;·&nbsp; "));
            md.push('\n');
        }

        md.push_str("\n**Annotations**\n");
        for a in anns {
            md.push_str(&format!("- `{}`\n", format_annotation_for_hover(a)));
        }

        // Tags
        let tags: Vec<(String, String)> = anns
            .iter()
            .filter(|a| a.name == "tag")
            .filter_map(|a| match &a.arg {
                AnnotationArg::String(s) if s.contains('=') => {
                    let (k, v) = s.split_once('=').unwrap();
                    Some((k.trim().to_string(), v.trim().trim_matches('"').to_string()))
                }
                _ => None,
            })
            .collect();
        if !tags.is_empty() {
            md.push_str("\n**Tags**\n");
            for (k, v) in tags {
                md.push_str(&format!("- *{}* = `{}`\n", k, v));
            }
        }
    }

    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: Some(span_to_range(span, &state.line_index)),
    }
}

fn resolve_value<'a>(
    config: &'a std::collections::BTreeMap<String, TpValue>,
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

fn format_value_display(v: &TpValue) -> String {
    match v {
        TpValue::String(s)  => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        TpValue::Integer(n) => n.to_string(),
        TpValue::Float(f)   => f.to_string(),
        TpValue::Bool(b)    => b.to_string(),
        TpValue::Null       => "null".into(),
        TpValue::Array(xs)  => {
            let inner: Vec<String> = xs.iter().take(6).map(format_value_display).collect();
            let suffix = if xs.len() > 6 { ", …" } else { "" };
            format!("[{}{}]", inner.join(", "), suffix)
        }
        TpValue::Dict(d) => {
            let inner: Vec<String> = d
                .iter()
                .take(4)
                .map(|(k, v)| format!("{} = {}", k, format_value_display(v)))
                .collect();
            let suffix = if d.len() > 4 { ", …" } else { "" };
            format!("#{{ {}{} }}#", inner.join(", "), suffix)
        }
    }
}

fn format_annotation_for_hover(a: &Annotation) -> String {
    match &a.arg {
        AnnotationArg::None      => format!("@{}", a.name),
        AnnotationArg::String(s) => format!("@{}: {}", a.name, s),
        AnnotationArg::Int(n)    => format!("@{}: {}", a.name, n),
        AnnotationArg::Float(f)  => format!("@{}: {}", a.name, f),
        AnnotationArg::List(xs)  => format!("@{}: [{}]", a.name, xs.join(", ")),
    }
}

fn builtin_doc(name: &str) -> &'static str {
    match name {
        "NOW"      => "\nCurrent UTC timestamp at parse time (ISO 8601).",
        "TODAY"    => "\nCurrent UTC date at parse time (YYYY-MM-DD).",
        "TRUE"     => "\nBoolean `true`.",
        "FALSE"    => "\nBoolean `false`.",
        "NULL"     => "\nThe null value.",
        "PID"      => "\nProcess ID of the parser.",
        "HOSTNAME" => "\nMachine hostname.",
        "PLATFORM" => "\nLowercased OS name (e.g. `windows`, `linux`).",
        "CWD"      => "\nCurrent working directory of the parser.",
        _ => "",
    }
}

// ── Completion ───────────────────────────────────────────────────────────────

fn completion_at(state: &DocState, pos: Position) -> Vec<CompletionItem> {
    let offset = pos_to_offset(pos, &state.line_index);
    let text = state.rope.to_string();
    let prefix = previous_token_prefix(&text, offset);

    if prefix.starts_with('@') {
        if line_starts_with(&text, offset, "@type:") {
            return type_completions();
        }
        return annotation_completions();
    }

    if prefix.starts_with('$') {
        return variable_completions(state);
    }

    Vec::new()
}

fn annotation_completions() -> Vec<CompletionItem> {
    KNOWN_ANNOTATIONS
        .iter()
        .map(|name| {
            let (insert, fmt, doc) = annotation_snippet(name);
            CompletionItem {
                label: format!("@{}", name),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("annotation".into()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: doc.into(),
                })),
                insert_text: Some(insert.into()),
                insert_text_format: Some(fmt),
                ..Default::default()
            }
        })
        .collect()
}

/// `(snippet, format, markdown-doc)` for each known annotation.
fn annotation_snippet(name: &str) -> (&'static str, InsertTextFormat, &'static str) {
    match name {
        "required"     => ("@required", InsertTextFormat::PLAIN_TEXT,
                           "**`@required`** — value must be present and non-empty."),
        "type"         => ("@type: ${1:int}", InsertTextFormat::SNIPPET,
                           "**`@type: T`** — declare the value's type. Common types: `string`, `int`, `float`, `bool`, `list`, `dict`, `url`, `email`, `duration`, `path`."),
        "min"          => ("@min: ${1:0}", InsertTextFormat::SNIPPET,
                           "**`@min: N`** — numeric value must be ≥ N."),
        "max"          => ("@max: ${1:100}", InsertTextFormat::SNIPPET,
                           "**`@max: N`** — numeric value must be ≤ N."),
        "minlen"       => ("@minlen: ${1:1}", InsertTextFormat::SNIPPET,
                           "**`@minlen: N`** — string/array/dict length must be ≥ N."),
        "maxlen"       => ("@maxlen: ${1:64}", InsertTextFormat::SNIPPET,
                           "**`@maxlen: N`** — string/array/dict length must be ≤ N."),
        "pattern"      => ("@pattern: \"${1:regex}\"", InsertTextFormat::SNIPPET,
                           "**`@pattern: \"regex\"`** — string must fully match the regex."),
        "enum"         => ("@enum: [${1:a, b, c}]", InsertTextFormat::SNIPPET,
                           "**`@enum: [a, b, c]`** — string must be one of these values."),
        "positive"     => ("@positive", InsertTextFormat::PLAIN_TEXT,
                           "**`@positive`** — numeric value must be > 0."),
        "nonzero"      => ("@nonzero", InsertTextFormat::PLAIN_TEXT,
                           "**`@nonzero`** — numeric value must be ≠ 0."),
        "nonempty"     => ("@nonempty", InsertTextFormat::PLAIN_TEXT,
                           "**`@nonempty`** — string/array/dict must be non-empty."),
        "deprecated"   => ("@deprecated(\"${1:replacement}\")", InsertTextFormat::SNIPPET,
                           "**`@deprecated(msg)`** — mark a key as deprecated. Surfaces as an editor warning."),
        "tag"          => ("@tag: ${1:owner} = \"${2:team}\"", InsertTextFormat::SNIPPET,
                           "**`@tag: k = \"v\"`** — attach arbitrary metadata (no validation)."),
        "internal"     => ("@internal", InsertTextFormat::PLAIN_TEXT,
                           "**`@internal`** — marks an implementation detail."),
        "readonly"     => ("@readonly", InsertTextFormat::PLAIN_TEXT,
                           "**`@readonly`** — value should not be modified at runtime."),
        "experimental" => ("@experimental", InsertTextFormat::PLAIN_TEXT,
                           "**`@experimental`** — subject to change."),
        _              => ("", InsertTextFormat::PLAIN_TEXT, ""),
    }
}

fn type_completions() -> Vec<CompletionItem> {
    KNOWN_TYPES
        .iter()
        .map(|ty| CompletionItem {
            label: (*ty).into(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            detail: Some("type".into()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: type_doc(ty).into(),
            })),
            insert_text: Some((*ty).into()),
            ..Default::default()
        })
        .collect()
}

fn type_doc(ty: &str) -> &'static str {
    match ty {
        "string"       => "UTF-8 string.",
        "int"          => "Signed 64-bit integer (dec/hex/oct/bin literals OK).",
        "float"        => "IEEE-754 double (accepts integer literals too).",
        "bool"         => "`true` or `false`.",
        "dict"         => "Inline or block dictionary `#{ … }#`.",
        "list"         => "Array `[…]`.",
        "list[string]" => "Array whose every element is a string.",
        "list[int]"    => "Array whose every element is an integer.",
        "list[float]"  => "Array whose every element is numeric.",
        "list[bool]"   => "Array of booleans.",
        "url"          => "String beginning with `http://` or `https://`.",
        "email"        => "String matching a relaxed email regex.",
        "path"         => "String (filesystem path — not checked for existence).",
        "duration"     => "Duration string like `30s`, `5m`, `2h`, `1d`.",
        _              => "",
    }
}

fn variable_completions(state: &DocState) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // User vars
    for (name, value) in &state.parsed.vars {
        items.push(CompletionItem {
            label: format!("${}", name),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some(format!("user variable · {}", value.type_name())),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```tomlplus\n{} = {}\n```", name, format_value_display(value)),
            })),
            ..Default::default()
        });
    }

    // Built-ins
    for b in BUILTIN_VARS {
        items.push(CompletionItem {
            label: format!("${}", b),
            kind: Some(CompletionItemKind::CONSTANT),
            detail: Some("built-in".into()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: builtin_doc(b).into(),
            })),
            ..Default::default()
        });
    }

    // $ENV.X scaffold
    items.push(CompletionItem {
        label: "$ENV.".into(),
        kind: Some(CompletionItemKind::VARIABLE),
        detail: Some("environment variable".into()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "Reference an environment variable. Use `??` for a fallback when unset: `$ENV.PORT ?? 8080`.".into(),
        })),
        insert_text: Some("$ENV.${1:VAR_NAME}".into()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    });

    items
}

fn previous_token_prefix(text: &str, offset: usize) -> String {
    let bytes = text.as_bytes();
    let mut start = offset.min(bytes.len());
    while start > 0 {
        let b = bytes[start - 1];
        if b == b'@' || b == b'$' {
            start -= 1;
            break;
        }
        if b.is_ascii_whitespace() || b == b'=' || b == b',' || b == b'(' {
            break;
        }
        start -= 1;
    }
    text[start..offset.min(text.len())].to_string()
}

fn line_starts_with(text: &str, offset: usize, needle: &str) -> bool {
    let line_start = text[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line = &text[line_start..offset];
    line.trim_start().starts_with(needle)
}

// ── Document symbols ─────────────────────────────────────────────────────────

fn document_symbols(state: &DocState) -> Vec<DocumentSymbol> {
    fn dict_to_symbols(
        d: &std::collections::BTreeMap<String, TpValue>,
        prefix: &str,
        spans: &std::collections::HashMap<String, Span>,
        idx: &LineIndex,
    ) -> Vec<DocumentSymbol> {
        let mut out = Vec::new();
        for (k, v) in d {
            let fqk = if prefix.is_empty() { k.clone() } else { format!("{}.{}", prefix, k) };
            let span = spans.get(&fqk).copied().unwrap_or(Span::DUMMY);
            let range = span_to_range(span, idx);
            let (kind, children) = match v {
                TpValue::Dict(inner) => (
                    SymbolKind::NAMESPACE,
                    Some(dict_to_symbols(inner, &fqk, spans, idx)),
                ),
                TpValue::Array(_)   => (SymbolKind::ARRAY, None),
                TpValue::String(_)  => (SymbolKind::STRING, None),
                TpValue::Integer(_) | TpValue::Float(_) => (SymbolKind::NUMBER, None),
                TpValue::Bool(_)    => (SymbolKind::BOOLEAN, None),
                TpValue::Null       => (SymbolKind::NULL, None),
            };
            #[allow(deprecated)]
            out.push(DocumentSymbol {
                name: k.clone(),
                detail: Some(symbol_detail(v)),
                kind,
                tags: None,
                deprecated: None,
                range,
                selection_range: range,
                children,
            });
        }
        out
    }
    dict_to_symbols(&state.parsed.config, "", &state.parsed.key_spans, &state.line_index)
}

fn symbol_detail(v: &TpValue) -> String {
    match v {
        TpValue::Dict(d)  => format!("dict · {} keys", d.len()),
        TpValue::Array(a) => format!("list · {} items", a.len()),
        _ => v.type_name().to_string(),
    }
}

// ── Goto definition ──────────────────────────────────────────────────────────

fn definition_at(
    state: &DocState,
    uri: &Url,
    pos: Position,
) -> Option<GotoDefinitionResponse> {
    let offset = pos_to_offset(pos, &state.line_index);
    if let Some(vref) = state.parsed.var_refs.iter().find(|r| r.span.contains(offset)) {
        if matches!(vref.kind, VarRefKind::User) {
            if let Some(def_span) = state.parsed.var_def_spans.get(&vref.name) {
                let range = span_to_range(*def_span, &state.line_index);
                return Some(GotoDefinitionResponse::Scalar(Location {
                    uri: uri.clone(),
                    range,
                }));
            }
        }
    }
    None
}

// ── Semantic tokens ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct AbsToken {
    line: u32,
    col: u32,
    len: u32,
    ty: u32,
    modi: u32,
}

fn semantic_tokens(state: &DocState) -> Vec<SemanticToken> {
    let idx = &state.line_index;
    let mut toks: Vec<AbsToken> = Vec::new();

    // Sections + keys
    for (key, span) in &state.parsed.key_spans {
        let (line, col) = idx.position(span.start);
        let len = span.len() as u32;
        let value = resolve_value(&state.parsed.config, key);
        let is_section = matches!(value, Some(TpValue::Dict(_)));
        let ty = if is_section { tok::NAMESPACE } else { tok::PROPERTY };

        let mut modi_bits = modi::DECLARATION;
        if let Some(anns) = state.parsed.meta.get(key) {
            if anns.iter().any(|a| a.name == "deprecated") {
                modi_bits |= modi::DEPRECATED;
            }
            if anns.iter().any(|a| a.name == "readonly") {
                modi_bits |= modi::READONLY;
            }
        }

        toks.push(AbsToken { line, col, len, ty, modi: modi_bits });
    }

    // Variable references
    for vref in &state.parsed.var_refs {
        let (line, col) = idx.position(vref.span.start);
        let len = vref.span.len() as u32;
        let (ty, modi_bits) = match vref.kind {
            VarRefKind::User    => (tok::VARIABLE, 0u32),
            VarRefKind::Builtin => (tok::_CONSTANT, modi::READONLY | modi::STATIC | modi::DEFAULT_LIB),
            VarRefKind::Env     => (tok::_PARAMETER, modi::READONLY),
        };
        toks.push(AbsToken { line, col, len, ty, modi: modi_bits });
    }

    // Annotations — emit per-annotation @name + arg sub-tokens.
    for anns in state.parsed.meta.values() {
        for a in anns {
            let (l, c) = idx.position(a.name_span.start);
            toks.push(AbsToken {
                line: l,
                col: c,
                len: a.name_span.len() as u32,
                ty: tok::DECORATOR,
                modi: 0,
            });

            // Argument sub-tokens by annotation kind.
            if let Some(arg_span) = a.arg_span {
                let (al, ac) = idx.position(arg_span.start);
                let arg_len = arg_span.len() as u32;
                match a.name.as_str() {
                    "type" => toks.push(AbsToken {
                        line: al,
                        col: ac,
                        len: arg_len,
                        ty: tok::TYPE,
                        modi: 0,
                    }),
                    "enum" => {
                        // Per-element ENUM_MEMBER tokens (preferred over the whole list span).
                        for item in &a.list_item_spans {
                            let (il, ic) = idx.position(item.start);
                            toks.push(AbsToken {
                                line: il,
                                col: ic,
                                len: item.len() as u32,
                                ty: tok::ENUM_MEMBER,
                                modi: 0,
                            });
                        }
                    }
                    "min" | "max" | "minlen" | "maxlen" => toks.push(AbsToken {
                        line: al,
                        col: ac,
                        len: arg_len,
                        ty: tok::_NUMBER,
                        modi: 0,
                    }),
                    "pattern" => toks.push(AbsToken {
                        line: al,
                        col: ac,
                        len: arg_len,
                        ty: tok::_REGEXP,
                        modi: 0,
                    }),
                    "deprecated" => toks.push(AbsToken {
                        line: al,
                        col: ac,
                        len: arg_len,
                        ty: tok::_STRING,
                        modi: modi::DEPRECATED,
                    }),
                    _ => {}
                }
            }
        }
    }

    encode_delta(&mut toks)
}

fn encode_delta(toks: &mut [AbsToken]) -> Vec<SemanticToken> {
    toks.sort_by(|a, b| (a.line, a.col).cmp(&(b.line, b.col)));

    let mut out = Vec::with_capacity(toks.len());
    let mut prev_line = 0u32;
    let mut prev_col = 0u32;
    for t in toks {
        let delta_line  = t.line - prev_line;
        let delta_start = if delta_line == 0 { t.col - prev_col } else { t.col };
        out.push(SemanticToken {
            delta_line,
            delta_start,
            length: t.len,
            token_type: t.ty,
            token_modifiers_bitset: t.modi,
        });
        prev_line = t.line;
        prev_col  = t.col;
    }
    out
}

// ── Inlay hints ──────────────────────────────────────────────────────────────

fn inlay_hints(state: &DocState, _range: Range) -> Vec<InlayHint> {
    let idx = &state.line_index;
    let mut hints = Vec::new();

    // Resolved value of `$ENV.X` (when the env var is set) — use `→` so the
    // hint doesn't read like a second assignment.
    for vref in &state.parsed.var_refs {
        if !matches!(vref.kind, VarRefKind::Env) {
            continue;
        }
        if let Ok(val) = std::env::var(&vref.name) {
            let (line, col) = idx.position(vref.span.end);
            hints.push(InlayHint {
                position: Position::new(line, col),
                label: InlayHintLabel::String(format!(" → \"{}\"", elide(&val, 32))),
                kind: Some(InlayHintKind::PARAMETER),
                text_edits: None,
                tooltip: Some(InlayHintTooltip::String(format!(
                    "Environment variable {} resolves to {:?}",
                    vref.name, val
                ))),
                padding_left: Some(false),
                padding_right: Some(false),
                data: None,
            });
        }
    }

    // Resolved value of `$user_var` (scalar only).
    for vref in &state.parsed.var_refs {
        if !matches!(vref.kind, VarRefKind::User) {
            continue;
        }
        if let Some(v) = state.parsed.vars.get(&vref.name) {
            if matches!(v, TpValue::Dict(_) | TpValue::Array(_)) {
                continue;
            }
            let (line, col) = idx.position(vref.span.end);
            hints.push(InlayHint {
                position: Position::new(line, col),
                label: InlayHintLabel::String(format!(
                    " → {}",
                    elide(&format_value_display(v), 32)
                )),
                kind: Some(InlayHintKind::PARAMETER),
                text_edits: None,
                tooltip: None,
                padding_left: Some(false),
                padding_right: Some(false),
                data: None,
            });
        }
    }

    // For each annotated key with @type:, show "(T)" inline next to the value.
    for (key, anns) in &state.parsed.meta {
        if let Some(t) = anns.iter().find_map(|a| match (&a.name[..], &a.arg) {
            ("type", AnnotationArg::String(s)) => Some(s.clone()),
            _ => None,
        }) {
            if let Some(span) = state.parsed.value_spans.get(key) {
                let (line, col) = idx.position(span.end);
                hints.push(InlayHint {
                    position: Position::new(line, col),
                    label: InlayHintLabel::String(format!(": {}", t)),
                    kind: Some(InlayHintKind::TYPE),
                    text_edits: None,
                    tooltip: None,
                    padding_left: Some(true),
                    padding_right: Some(false),
                    data: None,
                });
            }
        }
    }

    hints
}

fn elide(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max - 1).collect();
        format!("{}…", head)
    }
}

// ── Color provider ───────────────────────────────────────────────────────────

fn document_colors(state: &DocState) -> Vec<ColorInformation> {
    let idx = &state.line_index;
    let mut out = Vec::new();

    fn walk(
        v: &TpValue,
        prefix: &str,
        value_spans: &std::collections::HashMap<String, Span>,
        idx: &LineIndex,
        out: &mut Vec<ColorInformation>,
    ) {
        match v {
            TpValue::String(s) => {
                if let Some(c) = parse_color(s) {
                    if let Some(span) = value_spans.get(prefix) {
                        // Highlight the whole literal (incl. quotes) for click area.
                        let range = span_to_range(*span, idx);
                        out.push(ColorInformation { range, color: c });
                    }
                }
            }
            TpValue::Dict(d) => {
                for (k, vv) in d {
                    let next = if prefix.is_empty() { k.clone() } else { format!("{}.{}", prefix, k) };
                    walk(vv, &next, value_spans, idx, out);
                }
            }
            _ => {}
        }
    }

    for (k, v) in &state.parsed.config {
        walk(v, k, &state.parsed.value_spans, idx, &mut out);
    }
    out
}

fn parse_color(s: &str) -> Option<Color> {
    // #RGB / #RRGGBB / #RRGGBBAA
    let s = s.trim();
    let hex = s.strip_prefix('#')?;
    let (r, g, b, a) = match hex.len() {
        3 => (
            u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?,
            u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?,
            u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?,
            255,
        ),
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            255,
        ),
        8 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            u8::from_str_radix(&hex[6..8], 16).ok()?,
        ),
        _ => return None,
    };
    Some(Color {
        red: r as f32 / 255.0,
        green: g as f32 / 255.0,
        blue: b as f32 / 255.0,
        alpha: a as f32 / 255.0,
    })
}

fn format_color_hex(c: &Color) -> String {
    let r = (c.red   * 255.0).round() as u8;
    let g = (c.green * 255.0).round() as u8;
    let b = (c.blue  * 255.0).round() as u8;
    let a = (c.alpha * 255.0).round() as u8;
    if a == 255 {
        format!("\"#{:02X}{:02X}{:02X}\"", r, g, b)
    } else {
        format!("\"#{:02X}{:02X}{:02X}{:02X}\"", r, g, b, a)
    }
}
