//! Help and version output.

/// Print the command-line help.
pub(crate) fn print_help() {
    println!(
        "Kiru Compiler: The compiler for Kiru programming language.\n\n\
         Usage: kc [PATH] [-o PATH]\n\
         \x20      kc help\n\
         \x20      kc version\n\n\
         Arguments:\n\
         \x20 [PATH]  Path to the entry file\n\n\
         Options:\n\
         \x20 -o <PATH>  Output path; defaults to the entry path without the `.kiru` suffix"
    );
}

/// Print the compiler version.
pub(crate) fn print_version() {
    println!("kc {}", env!("CARGO_PKG_VERSION"));
}
