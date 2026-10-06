//! The output natives: writing to a standard stream and asking whether one is
//! a terminal.
//!
//! `is_terminal` answers with text, since the language has no boolean kind.

use std::io::{IsTerminal, Read, Write};

use crate::model::Value;

use super::boolean_text;

/// Write `text` to the standard stream named by `file_descriptor`: `"1"` is
/// stdout and `"2"` is stderr.
pub(crate) fn write(file_descriptor: &str, text: &str) -> Result<Value, String> {
    match file_descriptor {
        "1" => write_stream(&mut std::io::stdout(), text)?,
        "2" => write_stream(&mut std::io::stderr(), text)?,
        other => return Err(unknown_file_descriptor(other)),
    }
    Ok(Value::Nothing)
}

/// Whether the standard stream named by `file_descriptor` is a terminal.
pub(crate) fn is_terminal(file_descriptor: &str) -> Result<Value, String> {
    let terminal = match file_descriptor {
        "1" => std::io::stdout().is_terminal(),
        "2" => std::io::stderr().is_terminal(),
        other => return Err(unknown_file_descriptor(other)),
    };
    Ok(Value::Text(boolean_text(terminal)))
}

/// Read all of standard input as text.
pub(crate) fn read_stdin() -> Result<Value, String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| error.to_string())?;
    Ok(Value::Text(text))
}

fn write_stream(stream: &mut impl Write, text: &str) -> Result<(), String> {
    stream
        .write_all(text.as_bytes())
        .and_then(|()| stream.flush())
        .map_err(|error| error.to_string())
}

fn unknown_file_descriptor(file_descriptor: &str) -> String {
    format!("unknown file descriptor `{file_descriptor}`; expected \"1\" or \"2\"")
}
