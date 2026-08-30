use std::path::PathBuf;

fn resolve_compile_input(input: Option<PathBuf>) -> PathBuf {
    if let Some(path) = input {
        return path;
    }
    if crate::exec::kiru_cwd_enabled() {
        return PathBuf::from("main.kiru");
    }
    super::kiru_config_dir().join("main.kiru")
}

fn resolve_compile_output(output: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = output {
        return dir.join("kirufile");
    }
    if crate::exec::kiru_cwd_enabled() {
        return PathBuf::from("kirufile");
    }
    super::kiru_config_dir().join("kirufile")
}

pub fn run_compile_command(input: Option<PathBuf>, output: Option<PathBuf>) -> Result<(), String> {
    let input = resolve_compile_input(input);
    let output = resolve_compile_output(output);

    let ir = crate::lower::lower_and_resolve(&input, crate::exec::kiru_cwd_enabled())
        .map_err(super::compile_error_to_string)?;

    let text = ir.serialize();
    std::fs::write(&output, text)
        .map_err(|e| format!("failed to write kirufile {}: {}", output.display(), e))?;

    println!("compiled {} -> {}", input.display(), output.display());
    Ok(())
}
