//! Help and version output.

/// Print the command-line help.
pub(crate) fn print_help() {
    println!(
        "Kiru Compiler<The> compiler for Kiru programming language.\n\n\
         Usage: kc [kiru.toml] [-o PATH]\n\
         \x20      kc lsp\n\
         \x20      kc help\n\
         \x20      kc version\n\n\
         Arguments:\n\
         \x20 [kiru.toml]  Project manifest; defaults to ./kiru.toml\n\n\
         Options:\n\
         \x20 -o <PATH>  Output path; defaults to the manifest's project.output"
    );
}

/// Print the compiler version.
pub(crate) fn print_version() {
    println!("kc {}", env!("CARGO_PKG_VERSION"));
}
