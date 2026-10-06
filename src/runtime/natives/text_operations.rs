//! The text natives: length, slicing, search, splitting, and trimming.
//!
//! Length and slicing count bytes, not characters, so they are constant time
//! and line up with files and shell byte streams. Slicing is half-open and
//! fails the run on an index outside the text or one that splits a character.

use crate::model::Value;

use super::parse_whole_number;

/// The number of bytes in `text`.
pub(crate) fn length(text: &str) -> Value {
    Value::Text(text.len().to_string())
}

/// The bytes of `text` in the half-open range `start..end`. An index outside
/// the text, or one that splits a character, fails the run.
pub(crate) fn slice(text: &str, start: &str, end: &str) -> Result<Value, String> {
    let start = parse_whole_number(start)?;
    let end = parse_whole_number(end)?;
    if start > end || end > text.len() {
        return Err(format!(
            "range {start}..{end} is outside text of {} byte(s)",
            text.len()
        ));
    }
    if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return Err(format!("range {start}..{end} splits a character"));
    }
    Ok(Value::Text(text[start..end].to_owned()))
}

/// The byte index of the first occurrence of `needle`, or `""` when it is
/// absent.
pub(crate) fn find(text: &str, needle: &str) -> Value {
    Value::Text(
        text.find(needle)
            .map(|index| index.to_string())
            .unwrap_or_default(),
    )
}

/// `text` without leading and trailing whitespace.
pub(crate) fn trim(text: &str) -> Value {
    Value::Text(text.trim().to_owned())
}

/// Split `text` into a list of elements separated by `separator`.
pub(crate) fn split(text: &str, separator: &str) -> Value {
    Value::List(text.split(separator).map(str::to_owned).collect())
}

/// Split `text` into a list of lines. A trailing newline does not produce a
/// trailing empty element.
pub(crate) fn lines(text: &str) -> Value {
    Value::List(text.lines().map(str::to_owned).collect())
}
