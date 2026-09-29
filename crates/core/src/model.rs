use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Edit {
    pub start_byte: usize,
    pub end_byte: usize,
    pub replacement: String,
    pub rule_id: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub start_byte: usize,
    pub end_byte: usize,
    pub message: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Analysis {
    pub edits: Vec<Edit>,
    pub diagnostics: Vec<Diagnostic>,
}
