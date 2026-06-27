use std::io::BufRead;
use std::process::{Command, Stdio};

use super::colors;
use crate::dsl::Expr;
use crate::runner::OutputCallback;
use crate::runner::error::RuntimeError;
use crate::runner::output::OutputTarget;
use crate::shell;

use super::parse::ExecContext;

impl ExecContext<'_> {
    pub(super) fn exec_command(&mut self, value: &Expr) -> Result<(), RuntimeError> {
        let cmd_str = self.resolve_expr(value)?;
        let indent = self.indent(0);
        let line = format!("{}exec {}", indent, cmd_str);
        self.output
            .writeln_colored(&line, colors::EXEC_ANSI)
            .map_err(RuntimeError::Io)?;

        let shell = shell::current_shell();
        let mut child = Command::new(&shell)
            .arg("-c")
            .arg(&cmd_str)
            .current_dir(&self.work_dir)
            .envs(self.build_env())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RuntimeError::exec_io_error(&cmd_str, e))?;

        let indent = self.indent(1);

        let status = match self.output.clone_callback() {
            Some(cb) => {
                let stdout_thread =
                    spawn_stream_reader(child.stdout.take(), indent.clone(), cb.clone());
                let stderr_thread = spawn_stream_reader(child.stderr.take(), indent, cb);

                let status = child
                    .wait()
                    .map_err(|e| RuntimeError::exec_io_error(&cmd_str, e))?;

                if let Some(result) = stdout_thread.map(|h| h.join()) {
                    result
                        .map_err(|_| RuntimeError::Panic("stdout reader panicked".to_string()))??;
                }
                if let Some(result) = stderr_thread.map(|h| h.join()) {
                    result
                        .map_err(|_| RuntimeError::Panic("stderr reader panicked".to_string()))??;
                }

                status
            }
            None => {
                let output = child
                    .wait_with_output()
                    .map_err(|e| RuntimeError::exec_io_error(&cmd_str, e))?;
                write_output_lines(self.output, &output.stdout, &indent)?;
                write_output_lines(self.output, &output.stderr, &indent)?;
                output.status
            }
        };

        if !status.success() {
            return Err(RuntimeError::exec_exit_code(cmd_str, status.code()));
        }

        Ok(())
    }
}

fn spawn_stream_reader<R: std::io::Read + Send + 'static>(
    stream: Option<R>,
    indent: String,
    cb: OutputCallback,
) -> Option<std::thread::JoinHandle<Result<(), RuntimeError>>> {
    stream.map(|s| {
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(s);
            for line in reader.lines() {
                let line = line.map_err(RuntimeError::Io)?;
                cb([indent.as_str(), line.as_str()].concat());
            }
            Ok(())
        })
    })
}

fn write_output_lines(
    output: &mut OutputTarget,
    data: &[u8],
    indent: &str,
) -> Result<(), RuntimeError> {
    for line in std::io::BufReader::new(data).lines() {
        let line = line.map_err(RuntimeError::Io)?;
        output
            .writeln(&[indent, &line].concat())
            .map_err(RuntimeError::Io)?;
    }
    Ok(())
}
