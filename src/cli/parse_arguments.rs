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
    /// Serve the language protocol on standard input and output.
    Serve { entry: Option<PathBuf> },
}

/// Parse the words after the program name. `help` and `version` win wherever
/// they appear, before any later word is read. The first other word is the
/// entry path; `-o` sets the output path.
pub(crate) fn parse(words: &[String]) -> Result<Request, CliError> {
    // The server takes the first word, so a path named `help` after it is a
    // path, not a request for usage.
    if words.first().map(String::as_str) == Some("lsp") {
        let mut rest = words[1..].iter();
        let entry = rest.next().map(PathBuf::from);
        if let Some(word) = rest.next() {
            return Err(CliError(format!(
                "unexpected argument `{word}`; run `kc help` for usage"
            )));
        }
        return Ok(Request::Serve { entry });
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(words: &[&str]) -> Result<Request, CliError> {
        parse(
            &words
                .iter()
                .map(|word| word.to_string())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn the_server_takes_an_optional_entry() {
        assert!(matches!(
            parsed(&["lsp"]),
            Ok(Request::Serve { entry: None })
        ));
        assert!(matches!(
            parsed(&["lsp", "main.kiru"]),
            Ok(Request::Serve { entry: Some(entry) }) if entry == std::path::Path::new("main.kiru")
        ));
        assert!(
            parsed(&["lsp", "a.kiru", "b.kiru"]).is_err(),
            "one entry at most"
        );
    }

    #[test]
    fn a_compilation_is_not_the_server() {
        assert!(matches!(
            parsed(&["main.kiru"]),
            Ok(Request::Compile { entry, output: None }) if entry == std::path::Path::new("main.kiru")
        ));
    }
}
