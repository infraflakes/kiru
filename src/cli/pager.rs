use std::io::Write;
use std::process::{Command, Stdio};

pub(crate) fn display_output_through_pager(output: &str) -> Result<(), String> {
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

fn pipe_to_pager(output: &str) -> Result<(), String> {
    let (pager, is_default) = match std::env::var("PAGER") {
        Ok(v) => (v, false),
        Err(_) => ("less".to_string(), true),
    };
    let pager_parts = shlex::split(&pager)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("failed to parse PAGER: '{}'", pager))?;
    let (program, rest) = pager_parts
        .split_first()
        .ok_or_else(|| format!("no pager command in PAGER='{}'", pager))?;
    let mut args: Vec<String> = rest.iter().map(|s| s.to_string()).collect();

    if is_default {
        args.push("-R".to_string());
    }

    let mut cmd = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn pager '{}': {}", pager, e))?;

    if let Some(mut stdin) = cmd.stdin.take() {
        stdin
            .write_all(output.as_bytes())
            .map_err(|e| format!("failed to write to pager: {}", e))?;
    }

    let status = cmd
        .wait()
        .map_err(|e| format!("pager exited with error: {}", e))?;

    if !status.success() {
        return Err(format!(
            "pager '{}' {}",
            pager,
            crate::exec::subprocess::describe_exit_failure(&status)
        ));
    }

    Ok(())
}
