use std::path::PathBuf;

/// Compile a `.kiru` DSL source file into a `kirufile` s-expression artifact.
///
/// The `.kiru` file (and any it `import`s) is fully compiled and resolved, then
/// serialized as the textual `kirufile` IR written to `output`. Runtime
/// commands (`run`/`status`/`sync`/`fn`) consume that artifact instead of the
/// DSL source.
pub fn run_compile_command(input: Option<PathBuf>, output: Option<PathBuf>) -> miette::Result<()> {
    let input = input.unwrap_or_else(|| PathBuf::from("main.kiru"));
    let output = output.unwrap_or_else(|| PathBuf::from("kirufile"));

    let plan = crate::compiler::compile_and_resolve(&input, crate::runner::kiru_cwd_enabled())
        .map_err(super::compile_error_to_report)?;

    let text = plan.to_kirufile();
    std::fs::write(&output, text)
        .map_err(|e| miette::miette!("failed to write kirufile {}: {}", output.display(), e))?;

    println!("compiled {} -> {}", input.display(), output.display());
    Ok(())
}
