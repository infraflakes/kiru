use std::path::PathBuf;

/// Resolve the compile input path.
///
/// Resolution order:
/// 1. Explicit `input` positional → use it.
/// 2. `KIRU_CWD=1` → `main.kiru` in the current directory.
/// 3. Default → `~/.config/kiru/main.kiru`.
fn resolve_compile_input(input: Option<PathBuf>) -> PathBuf {
    if let Some(path) = input {
        return path;
    }
    if crate::exec::kiru_cwd_enabled() {
        return PathBuf::from("main.kiru");
    }
    super::kiru_config_dir().join("main.kiru")
}

/// Resolve the compile output path.
///
/// Resolution order:
/// 1. Explicit `-o` directory → `<dir>/kirufile`.
/// 2. `KIRU_CWD=1` → `kirufile` in the current directory.
/// 3. Default → `~/.config/kiru/kirufile`.
fn resolve_compile_output(output: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = output {
        return dir.join("kirufile");
    }
    if crate::exec::kiru_cwd_enabled() {
        return PathBuf::from("kirufile");
    }
    super::kiru_config_dir().join("kirufile")
}

/// Compile a `.kiru` DSL source file into a `kirufile` s-expression artifact.
///
/// The `.kiru` file (and any it `import`s) is fully compiled and resolved, then
/// serialized as the textual `kirufile` IR written to `output`. Runtime
/// commands (`run`/`status`/`sync`/`fn`) consume that artifact instead of the
/// DSL source.
pub fn run_compile_command(
    input: Option<PathBuf>,
    output: Option<PathBuf>,
) -> miette::Result<()> {
    let input = resolve_compile_input(input);
    let output = resolve_compile_output(output);

    let ir = crate::lower::lower_and_resolve(&input, crate::exec::kiru_cwd_enabled())
        .map_err(super::compile_error_to_report)?;

    let text = ir.serialize();
    std::fs::write(&output, text)
        .map_err(|e| miette::miette!("failed to write kirufile {}: {}", output.display(), e))?;

    println!("compiled {} -> {}", input.display(), output.display());
    Ok(())
}
