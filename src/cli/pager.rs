use std::io::Write;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

/// Display output to stdout, automatically piping through `$PAGER` when the
/// output is longer than the terminal height and stdout is a terminal.
pub(crate) fn display_output_through_pager(output: &str) -> miette::Result<()> {
    use std::io::IsTerminal;

    let use_pager = std::io::stdout().is_terminal()
        && crossterm::terminal::size()
            .ok()
            .is_some_and(|(_, h)| output.lines().count() > h as usize);

    if use_pager {
        pipe_to_pager(output)
    } else {
        print!("{}", output);
        Ok(())
    }
}

/// Pipe output through `$PAGER` (defaults to `less -R`).
fn pipe_to_pager(output: &str) -> miette::Result<()> {
    let (pager, is_default) = match std::env::var("PAGER") {
        Ok(v) => (v, false),
        Err(_) => ("less".to_string(), true),
    };
    let pager_parts = shlex::split(&pager)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| miette::miette!("failed to parse PAGER: '{}'", pager))?;
    let (program, rest) = pager_parts
        .split_first()
        .ok_or_else(|| miette::miette!("no pager command in PAGER='{}'", pager))?;
    let mut args: Vec<String> = rest.iter().map(|s| s.to_string()).collect();

    if is_default {
        args.push("-R".to_string());
    }

    let mut cmd = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| miette::miette!("failed to spawn pager '{}': {}", pager, e))?;

    if let Some(mut stdin) = cmd.stdin.take() {
        stdin
            .write_all(output.as_bytes())
            .map_err(|e| miette::miette!("failed to write to pager: {}", e))?;
    }

    let status = cmd
        .wait()
        .map_err(|e| miette::miette!("pager exited with error: {}", e))?;

    if !status.success() {
        if let Some(signal) = status.signal() {
            return Err(miette::miette!(
                "pager '{}' was terminated by signal {}",
                pager,
                signal
            ));
        }
        return Err(miette::miette!(
            "pager '{}' exited with code {:?}",
            pager,
            status.code()
        ));
    }

    Ok(())
}
