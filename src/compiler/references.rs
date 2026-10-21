//! The references a declaration earns.
//!
//! After the check, every place the program names a declaration is an edge:
//! a `Reference` node the bodies hold, and the binding an assignment names.
//! The references a declaration earns are the places those edges write its
//! name at, by file — the reverse of what a definition answers. The facts
//! are the reads the program already recorded, so the query is one pass over
//! them.

use std::path::{Path, PathBuf};

use crate::compiler::build::Build;
use crate::compiler::model::UseTarget;
use crate::compiler::reads::collect_reads;
use crate::syntax::Span;

impl Build {
    /// The references of the declaration a position in `path` stands on: the
    /// written name of every read and every write that names it, by file,
    /// and — when `include_declaration` — the declaration's own name. A
    /// position that names no declaration earns no reference.
    pub fn references(
        &self,
        path: &Path,
        offset: usize,
        include_declaration: bool,
    ) -> Vec<(PathBuf, Span)> {
        let Some((target, at_file)) = self.declaration_at(path, offset) else {
            return Vec::new();
        };
        let mut places = Vec::new();
        if include_declaration && let Some(file) = self.name_file(target, at_file) {
            let declaration = self.program.declaration(target);
            places.push((
                self.program.files[file.0].path.clone(),
                declaration.name_span,
            ));
        }
        let reads = collect_reads(&self.program);
        for read in reads.reads.iter().filter(|read| read.declaration == target) {
            let span = read.name_span(&self.loaded.files[read.file.0].source);
            places.push((self.program.files[read.file.0].path.clone(), span));
        }
        for write in reads
            .writes
            .iter()
            .filter(|write| write.declaration == target)
        {
            places.push((self.program.files[write.file.0].path.clone(), write.span));
        }
        for (use_index, delivered) in self.deliveries() {
            if delivered == UseTarget::Function(target) {
                let used = &self.loaded.uses[use_index];
                places.push((self.loaded.files[used.file].path.clone(), used.delivered));
            }
        }
        places
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{Build, VirtualSource, build};

    /// The build of a virtual program.
    fn built(files: &[(&str, &str)]) -> Build {
        build(Path::new("main.kiru"), &VirtualSource::new(files)).expect("the program checks")
    }

    /// The file the build read the entry by.
    fn entry(built: &Build) -> PathBuf {
        built.loaded.files[built.loaded.entry.expect("a built program names its entry")]
            .path
            .clone()
    }

    /// The references of `needle`'s first place, as (file, text) pairs.
    fn places(built: &Build, files: &[(&str, &str)], needle: &str) -> Vec<(String, String)> {
        let start = files[0].1.find(needle).expect("the needle is written");
        built
            .references(&entry(built), start, true)
            .into_iter()
            .map(|(path, span)| {
                let source = built
                    .loaded
                    .files
                    .iter()
                    .find(|file| file.path == path)
                    .expect("the file is loaded")
                    .source[span.start..span.end]
                    .to_owned();
                (path.to_string_lossy().to_string(), source)
            })
            .collect()
    }

    #[test]
    fn a_function_earns_its_declaration_and_every_read() {
        let files = [(
            "main.kiru",
            "fn speak() -> txt { return \"spoken\"; };\nfn main() { std::print(speak()); speak(); };",
        )];
        let program = built(&files);
        assert_eq!(
            places(&program, &files, "speak() -> txt"),
            vec![
                ("/virtual/main.kiru".to_owned(), "speak".to_owned()),
                ("/virtual/main.kiru".to_owned(), "speak".to_owned()),
                ("/virtual/main.kiru".to_owned(), "speak".to_owned()),
            ],
            "the declaration and both reads, the first printed's argument"
        );
    }

    #[test]
    fn a_local_earns_its_binding_reads_and_writes() {
        let files = [(
            "main.kiru",
            "fn main() { let count: int = 1; count = count + 1; return; };",
        )];
        let program = built(&files);
        assert_eq!(
            places(&program, &files, "count: int"),
            vec![
                ("/virtual/main.kiru".to_owned(), "count".to_owned()),
                ("/virtual/main.kiru".to_owned(), "count".to_owned()),
                ("/virtual/main.kiru".to_owned(), "count".to_owned()),
            ],
            "the binding's name, the write, and the read in the sum"
        );
    }

    #[test]
    fn a_native_earns_its_reads_and_no_declaration() {
        let files = [(
            "main.kiru",
            "fn main() { std::print(\"one\"); std::print(\"two\"); };",
        )];
        let program = built(&files);
        let printed = places(&program, &files, "std::print(\"one\")")
            .into_iter()
            .filter(|(file, _)| file.ends_with("main.kiru"))
            .collect::<Vec<_>>();
        assert_eq!(
            printed,
            vec![
                ("/virtual/main.kiru".to_owned(), "print".to_owned()),
                ("/virtual/main.kiru".to_owned(), "print".to_owned()),
            ],
            "the native has no declaration to name, and the entry's two reads point at it"
        );
    }

    #[test]
    fn a_position_that_names_nothing_earns_nothing() {
        let files = [("main.kiru", "fn main() { return; };")];
        let program = built(&files);
        let keyword = files[0].1.find("return").expect("the keyword is written");
        assert!(
            program
                .references(&entry(&program), keyword, true)
                .is_empty(),
            "a position in a keyword earns no reference"
        );
        let files = [(
            "main.kiru",
            "fn speak() -> txt { return \"spoken\"; };\nfn main() { std::print(speak()); };",
        )];
        let program = built(&files);
        let declaration = files[0].1.find("speak").expect("the name is written");
        assert_eq!(
            program
                .references(&entry(&program), declaration, false)
                .len(),
            1,
            "the declaration's own name is excluded when the caller asks so, its read still answers"
        );
        assert_eq!(
            places(&program, &files, "speak() -> txt"),
            vec![
                ("/virtual/main.kiru".to_owned(), "speak".to_owned()),
                ("/virtual/main.kiru".to_owned(), "speak".to_owned()),
            ],
            "and included when it asks so: the declaration and its one read"
        );
    }
}
