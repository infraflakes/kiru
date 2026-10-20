//! The text natives: byte access and byte search.
//!
//! Indices count bytes, not characters, so they line up with files and shell
//! byte streams. Everything above this byte kernel — trimming, splitting,
//! matching — is kiru.

use crate::runtime::natives::{option_none, option_some};
use crate::runtime::{Text, Value};

/// The byte at `index`, for `s.byte(index)`. An index outside the text, or a
/// negative one, fails the run. This is the floor's byte access: everything
/// above it — number parsing, scanning — is kiru.
pub(crate) fn byte(text: &str, index: i64) -> Result<Value, String> {
    let bytes = text.as_bytes();
    if index < 0 || index as usize >= bytes.len() {
        return Err(format!(
            "index {index} is outside text of {} byte(s)",
            bytes.len()
        ));
    }
    Ok(Value::Int(bytes[index as usize] as i64))
}

/// The byte index of the first occurrence of `needle`, or `std::Option.none`
/// when it is not present. There is no `-1` sentinel: a missing index is the
/// `none` variant, exactly as a program writes it.
pub(crate) fn find(text: &str, needle: &str) -> Value {
    match text.find(needle) {
        Some(index) => option_some(Value::Int(index as i64)),
        None => option_none(),
    }
}

/// The bytes in `[start, end)`, for `s.slice(start, end)`. A negative index, an
/// index outside the text, or `start > end` fails the run, exactly as `byte`
/// does; the range must also fall on character boundaries.
pub(crate) fn slice(text: &Text, start: i64, end: i64) -> Result<Value, String> {
    let len = text.byte_len();
    if start < 0 || end < 0 || start > end || end as usize > len {
        return Err(format!(
            "slice [{start}, {end}) is outside text of {len} byte(s)"
        ));
    }
    Ok(Value::Text(text.slice(start as usize, end as usize)?))
}
