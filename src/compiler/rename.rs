//! The rename of a name.
//!
//! A rename is one workspace edit: every place the declaration's name is
//! written at, per file, rewritten to the new name. The compiler validates
//! the result the way it validates any program: the renamed tree is built,
//! and the diagnostics it raises are the rename's rejection, their wording
//! the compiler's own. A declaration written in no file of the workspace — a
//! native, or a standard-library declaration the program embeds — renames
//! nothing, because there is no real text to rewrite.

use std::io;
use std::path::{Path, PathBuf};

use crate::compiler::build::Build;
use crate::compiler::model::{DeclarationId, FileId};
use crate::compiler::reads::collect_reads;
use crate::compiler::{ProgramSource, invalid_name, no_workspace_file};
use crate::syntax::{Span, TokenKind, lex};

/// The tree the build already read, standing for the renamed sources: the
/// entry locates at the path the build recorded, the tree collects every
/// file under the entry's tree root, and one file reads its rewritten text.
struct RenamedTree {
    files: Vec<(PathBuf, String)>,
    tree_root: PathBuf,
}

impl ProgramSource for RenamedTree {
    fn locate(&self, entry: &Path) -> io::Result<(PathBuf, PathBuf)> {
        self.files
            .iter()
            .find(|(path, _)| path == entry)
            .map(|_| (entry.to_path_buf(), self.tree_root.clone()))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{}", entry.display())))
    }

    fn collect(&self, tree_root: &Path) -> io::Result<Vec<PathBuf>> {
        Ok(self
            .files
            .iter()
            .map(|(path, _)| path.clone())
            .filter(|path| path.starts_with(tree_root))
            .collect())
    }

    fn read(&self, path: &Path) -> io::Result<String> {
        self.files
            .iter()
            .find(|(known, _)| known == path)
            .map(|(_, source)| source.clone())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{}", path.display())))
    }
}

/// The edits one rename makes: the file and the written names' spans, one
/// entry per file the rename touches.
#[derive(Debug)]
pub struct Rename {
    pub edits: Vec<(PathBuf, Vec<Span>)>,
}

impl Build {
    /// The rename of the name a position in `path` stands on, to `new_name`:
    /// the edit when the compiler accepts it, none when the position names no
    /// declaration, and the compiler's diagnostic when it rejects the new
    /// name — a duplicate in the target namespace, or a word that is not a
    /// name — or when the declaration is written in no file of the workspace.
    pub fn rename(
        &self,
        path: &Path,
        offset: usize,
        new_name: &str,
    ) -> Result<Option<Rename>, String> {
        let Some((target, at_file)) = self.declaration_at(path, offset) else {
            return Ok(None);
        };
        let declaration = self.program.declaration(target);
        let name = declaration.name.clone();
        // The declaration's own name must stand in real text: a native holds
        // no file, and a standard-library file is embedded, not a workspace
        // file the editor rewrites.
        let Some(file) = self.name_file(target, at_file) else {
            return Err(no_workspace_file(&name));
        };
        if !self.program.files[file.0].path.is_absolute() {
            return Err(no_workspace_file(&name));
        }
        // A name is one name word, by the lexer's own rule.
        if !is_a_name(new_name) {
            return Err(invalid_name(new_name));
        }
        let edits = self.edits(target, file);
        self.checked(&edits, new_name)
            .map(|()| Some(Rename { edits }))
    }

    /// The edits the rename writes: the declaration's own name, and the
    /// written name of every read and write that names it, grouped per file.
    fn edits(&self, target: DeclarationId, file: FileId) -> Vec<(PathBuf, Vec<Span>)> {
        let reads = collect_reads(&self.program);
        let mut edits = vec![(
            self.program.files[file.0].path.clone(),
            vec![self.program.declaration(target).name_span],
        )];
        let place = |edits: &mut Vec<(PathBuf, Vec<Span>)>, path: &Path, span: Span| match edits
            .iter_mut()
            .find(|(file, _)| file == path)
        {
            Some((_, spans)) => spans.push(span),
            None => edits.push((path.to_path_buf(), vec![span])),
        };
        for read in reads.reads.iter().filter(|read| read.declaration == target) {
            let span = read.name_span(&self.loaded.files[read.file.0].source);
            place(&mut edits, &self.program.files[read.file.0].path, span);
        }
        for write in reads
            .writes
            .iter()
            .filter(|write| write.declaration == target)
        {
            place(
                &mut edits,
                &self.program.files[write.file.0].path,
                write.span,
            );
        }
        for (use_index, delivered) in self.deliveries() {
            if delivered == crate::compiler::model::UseTarget::Function(target) {
                let used = &self.loaded.uses[use_index];
                place(
                    &mut edits,
                    &self.loaded.files[used.file].path,
                    used.delivered,
                );
            }
        }
        edits
    }

    /// The compiler's own check over the renamed tree: the build of every
    /// file the program read, with the edit's spans rewritten. The rename is
    /// accepted only when the renamed tree checks, and the first fact the
    /// build stopped on is the rejection.
    fn checked(&self, edits: &[(PathBuf, Vec<Span>)], new_name: &str) -> Result<(), String> {
        let spans_of = |path: &Path| -> Vec<Span> {
            edits
                .iter()
                .find(|(edit_path, _)| edit_path == path)
                .map(|(_, spans)| spans.clone())
                .unwrap_or_default()
        };
        let renamed = self
            .loaded
            .files
            .iter()
            .map(|file| {
                (
                    file.path.clone(),
                    rewritten(&file.source, &spans_of(&file.path), new_name),
                )
            })
            .collect();
        let entry = self.loaded.files[self.loaded.entry].path.clone();
        let source = RenamedTree {
            files: renamed,
            tree_root: entry.parent().unwrap_or(Path::new(".")).to_path_buf(),
        };
        match crate::compiler::build(&entry, &source) {
            Ok(_) => Ok(()),
            Err(failure) => {
                let message = failure
                    .diagnostics()
                    .iter()
                    .find_map(|file| file.diagnostics.first())
                    .map(|diagnostic| diagnostic.message.clone())
                    .unwrap_or_else(|| failure.rendered().to_owned());
                Err(message)
            }
        }
    }
}

/// The source with every span's text replaced by `new_name`, the spans
/// rewritten from the end so the earlier ones stay where they were.
fn rewritten(source: &str, spans: &[Span], new_name: &str) -> String {
    // The edits arrive in no order: the declaration first, then the reads,
    // then the delivered use names. Replacing from the end keeps every
    // earlier span's offsets valid, so the spans are ordered first.
    let mut ordered = spans.to_vec();
    ordered.sort_by_key(|span| span.start);
    let mut rewritten = source.to_owned();
    for span in ordered.iter().rev() {
        rewritten.replace_range(span.start..span.end, new_name);
    }
    rewritten
}

/// Whether the word lexes as one name: the lexer's own rule, checked by
/// running it.
fn is_a_name(word: &str) -> bool {
    match lex(word) {
        Ok(tokens) => {
            tokens.len() == 2
                && matches!(tokens[0].kind, TokenKind::Ident)
                && tokens[0].span == Span::new(0, word.len())
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{VirtualSource, build};

    /// The build of a virtual program.
    fn built(files: &[(&str, &str)]) -> Build {
        build(Path::new("main.kiru"), &VirtualSource::new(files)).expect("the program checks")
    }

    /// The file the build read the entry by.
    fn entry(built: &Build) -> PathBuf {
        built.loaded.files[built.loaded.entry].path.clone()
    }

    /// The renamed places: each file and the written names it holds.
    type Renamed = Result<Option<Vec<(String, Vec<String>)>>, String>;

    /// The rename of `offset`, as (file, text) pairs.
    fn renamed_at(
        program: &Build,
        files: &[(&str, &str)],
        offset: usize,
        new_name: &str,
    ) -> Renamed {
        program
            .rename(&entry(program), offset, new_name)
            .map(|rename| {
                rename.map(|rename| {
                    rename
                        .edits
                        .into_iter()
                        .map(|(path, spans)| {
                            (
                                path.file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string(),
                                spans
                                    .iter()
                                    .map(|span| files[0].1[span.start..span.end].to_owned())
                                    .collect(),
                            )
                        })
                        .collect()
                })
            })
    }

    /// The rename of `needle`'s first place, as (file, text) pairs.
    fn renamed(program: &Build, files: &[(&str, &str)], needle: &str, new_name: &str) -> Renamed {
        let start = files[0].1.find(needle).expect("the needle is written");
        renamed_at(program, files, start, new_name)
    }

    #[test]
    fn a_local_renames_every_place_its_name_is_written() {
        let files = [(
            "main.kiru",
            "fn main() { let count: int = 1; count = count + 1; return; };",
        )];
        let program = built(&files);
        assert_eq!(
            renamed(&program, &files, "count: int", "total"),
            Ok(Some(vec![(
                "main.kiru".to_owned(),
                vec!["count".to_owned(), "count".to_owned(), "count".to_owned(),],
            )])),
            "the binding, the write, and the read in the sum"
        );
    }

    #[test]
    fn a_use_delivered_function_renames_its_reads() {
        let files = [(
            "main.kiru",
            "use root::speak;\nfn speak() -> txt { return \"spoken\"; };\nfn main() { std::print(speak()); };",
        )];
        let program = built(&files);
        assert_eq!(
            renamed(&program, &files, "speak() -> txt", "say"),
            Ok(Some(vec![(
                "main.kiru".to_owned(),
                vec!["speak".to_owned(), "speak".to_owned(), "speak".to_owned()],
            )])),
            "the declaration, the use item's delivered name, and the read rename"
        );
    }

    #[test]
    fn a_duplicate_rejects_with_the_compiler_s_own_wording() {
        let files = [(
            "main.kiru",
            "fn speak() -> txt { return \"spoken\"; };\nfn main() { std::print(speak()); };",
        )];
        let program = built(&files);
        assert_eq!(
            renamed(&program, &files, "speak() -> txt", "main"),
            Err("`main` is declared more than once in this namespace".to_owned()),
        );
    }

    #[test]
    fn a_word_that_is_not_a_name_rejects() {
        let files = [("main.kiru", "fn main() { let x: int = 1; return; };")];
        let program = built(&files);
        assert_eq!(
            renamed(&program, &files, "x: int", "a b"),
            Err("`a b` is not a name".to_owned()),
        );
    }

    #[test]
    fn a_std_declaration_renames_nothing() {
        let files = [("main.kiru", "fn main() { std::print(\"spoken\"); };")];
        let program = built(&files);
        assert_eq!(
            renamed(&program, &files, "std::print(\"spoken\")", "echo"),
            Err("`print` is written in no file of the workspace".to_owned()),
        );
    }

    #[test]
    fn a_position_that_names_nothing_renames_nothing() {
        let files = [("main.kiru", "fn main() { return; };")];
        let program = built(&files);
        let keyword = files[0].1.find("return").expect("the keyword is written");
        assert_eq!(renamed_at(&program, &files, keyword, "leave"), Ok(None));
    }
}
