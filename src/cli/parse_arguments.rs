//! Parsing the words after the program name.

use std::path::PathBuf;

use super::CliError;

/// What the words ask for.
pub(crate) enum Request {
    /// Print usage.
    Help,
    /// Print the compiler version.
    Version,
    /// Compile the project the manifest names into its executable.
    Compile {
        manifest: PathBuf,
        output: Option<PathBuf>,
    },
    /// Serve the language protocol on standard input and output.
    Serve,
}

/// Parse the words after the program name. `help` and `version` win wherever
/// they appear, before any later word is read. The first other word is the
/// manifest; with no word, `./kiru.toml` is the project. `-o` sets the output
/// path.
pub(crate) fn parse(words: &[String]) -> Result<Request, CliError> {
    // The server takes the first word, so a path named `help` after it is a
    // path, not a request for usage.
    if words.first().map(String::as_str) == Some("lsp") {
        if let Some(word) = words.get(1) {
            return Err(CliError(format!(
                "unexpected argument `{word}`; the server takes no arguments; run `kc help` for usage"
            )));
        }
        return Ok(Request::Serve);
    }
    let mut manifest = None;
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
            word if manifest.is_none() => manifest = Some(PathBuf::from(word)),
            other => {
                return Err(CliError(format!(
                    "unexpected argument `{other}`; run `kc help` for usage"
                )));
            }
        }
        index += 1;
    }
    let manifest = manifest.unwrap_or_else(|| PathBuf::from("kiru.toml"));
    Ok(Request::Compile { manifest, output })
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
    fn the_server_takes_no_arguments() {
        assert!(matches!(parsed(&["lsp"]), Ok(Request::Serve)));
        assert!(
            parsed(&["lsp", "a.kiru"]).is_err(),
            "the file is the one the editor opens"
        );
    }

    #[test]
    fn the_manifest_is_the_target() {
        assert!(matches!(
            parsed(&["src/kiru.toml"]),
            Ok(Request::Compile { manifest, output: None })
                if manifest == std::path::Path::new("src/kiru.toml")
        ));
        assert!(matches!(
            parsed(&[]),
            Ok(Request::Compile { manifest, output: None })
                if manifest == std::path::Path::new("kiru.toml")
        ));
    }
}
