//! Pure Apex analysis and edit planning.
//!
//! Every source location is a UTF-8 byte offset.  This crate performs no IO.

pub mod model;
pub mod parser;

pub use model::{Analysis, Diagnostic, Edit, Severity};
pub use parser::analyze;

/// Applies a verified, non-overlapping edit plan from right to left.
pub fn apply_edits(source: &str, edits: &[Edit]) -> Result<String, &'static str> {
    let mut ordered = edits.to_vec();
    ordered.sort_by_key(|edit| (edit.start_byte, edit.end_byte));
    let mut last = 0;
    for edit in &ordered {
        if edit.start_byte > edit.end_byte || edit.end_byte > source.len() || edit.start_byte < last
        {
            return Err("invalid or overlapping edit plan");
        }
        last = edit.end_byte;
    }
    let mut output = source.to_owned();
    for edit in ordered.iter().rev() {
        output.replace_range(edit.start_byte..edit.end_byte, &edit.replacement);
    }
    Ok(output)
}
