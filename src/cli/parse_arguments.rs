//! Parsing the words after the program name.

use std::path::PathBuf;

use super::CliError;

/// What the words ask for.
pub(crate) enum Request {
    /// Print usage.
    Help,
    /// Print the compiler version.
    Version,
    /// Compile the entry file into an executable.
    Compile {
        entry: PathBuf,
        output: Option<PathBuf>,
    },
}

/// Parse the words after the program name. `help` and `version` win wherever
/// they appear, before any later word is read. The first other word is the
/// entry path; `-o` sets the output path.
pub(crate) fn parse(words: &[String]) -> Result<Request, CliError> {
    let mut entry = None;
    let mut output = None;
    let mut index = 0;
    while index < words.len() {
        match words[index].as_str() {
            "help" => return Ok(Request::Help),
            "version" => return Ok(Request::Version),
            "-o" => {
                index += 1;
                let Some(value) = words.get(index) else {
                    return Err(CliError("`-o` needs a path".to_owned()));
                };
                output = Some(PathBuf::from(value));
            }
            word if word.starts_with('-') => {
                return Err(CliError(format!(
                    "unexpected argument `{word}`; run `kc help` for usage"
                )));
            }
            word if entry.is_none() => entry = Some(PathBuf::from(word)),
            other => {
                return Err(CliError(format!(
                    "unexpected argument `{other}`; run `kc help` for usage"
                )));
            }
        }
        index += 1;
    }
    let Some(entry) = entry else {
        return Err(CliError("no input file, `kc help` for usage.".to_owned()));
    };
    Ok(Request::Compile { entry, output })
}
