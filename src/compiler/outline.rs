//! The outline of one file: the declarations it holds and the children they
//! declare.
//!
//! The model is the outline's own structure: a file's declarations are the
//! nodes the file joined, a declared type's fields and variants are its
//! body's members, and a bound function is filed under its type. An `impl`
//! block itself is no node: the loader flattens a block into its members, so
//! its functions stand in the outline beside the file's other declarations,
//! marked as bound.

use std::path::Path;

use crate::compiler::build::Build;
use crate::compiler::model::{DeclarationKind, FileId, Group, TypeKind};
use crate::syntax::Span;

/// One symbol of an outline: the declaration's name, the kind the model
/// marks it with, the span of its written name, and the children its body
/// declares.
#[derive(Debug)]
pub struct Outline {
    pub name: String,
    pub kind: OutlineKind,
    pub span: Span,
    pub children: Vec<Outline>,
}

/// What a declaration is, by the model's own structure. The boundary maps
/// these to the protocol's symbol kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutlineKind {
    /// A function declared at the namespace level.
    Function,
    /// A function filed under a type: reached through its type, whether it
    /// binds a receiver or not.
    Method,
    /// A module value: a name the namespace evaluates once.
    Value,
    /// A declared `rec` type.
    Rec,
    /// A declared `enum` type.
    Enum,
    /// One field of a declared `rec` type.
    Field,
    /// One variant of a declared `enum` type.
    Variant,
}

impl Build {
    /// The outline of the file `path` holds: every declaration of the file,
    /// in the order their names are written, with the children the model
    /// declares under them. A path the program reads no file by earns no
    /// outline.
    pub fn outline(&self, path: &Path) -> Vec<Outline> {
        let Some(file) = self.program.files.iter().position(|file| file.path == path) else {
            return Vec::new();
        };
        let file = FileId(file);
        let mut written: Vec<(usize, Outline)> = Vec::new();
        for id in &self.program.files[file.0].declarations {
            let declaration = self.program.declaration(*id);
            let kind = match declaration.kind {
                DeclarationKind::Function => OutlineKind::Function,
                // A member an impl block filed under its type: reached
                // through the type, never by a namespace-level name.
                DeclarationKind::Impl => OutlineKind::Method,
                DeclarationKind::ModuleValue => OutlineKind::Value,
                // A named type's name comes from the types table below; a
                // local is written in no file and a native has none.
                DeclarationKind::Rec | DeclarationKind::Enum => continue,
                DeclarationKind::Local | DeclarationKind::Native => {
                    unreachable!("a file's declarations introduce written names")
                }
            };
            written.push((
                declaration.name_span.start,
                Outline {
                    name: declaration.name.clone(),
                    kind,
                    span: declaration.name_span,
                    children: Vec::new(),
                },
            ));
        }
        let own_types = self
            .program
            .types
            .iter()
            .filter(|type_decl| type_decl.file == Some(file));
        for type_decl in own_types {
            let children = match &type_decl.kind {
                TypeKind::Rec(fields) => fields
                    .iter()
                    .map(|field| Outline {
                        name: field.name.clone(),
                        kind: OutlineKind::Field,
                        span: field.name_span,
                        children: Vec::new(),
                    })
                    .collect(),
                TypeKind::Enum(variants) => variants
                    .iter()
                    .map(|variant| Outline {
                        name: variant.name.clone(),
                        kind: OutlineKind::Variant,
                        span: variant.name_span,
                        children: Vec::new(),
                    })
                    .collect(),
            };
            let kind = match type_decl.group() {
                Group::Rec => OutlineKind::Rec,
                Group::Enum => OutlineKind::Enum,
                Group::Scalar | Group::Function | Group::Parameter => {
                    unreachable!("a declared type is a rec or an enum")
                }
            };
            written.push((
                type_decl.name_span.start,
                Outline {
                    name: type_decl.name.clone(),
                    kind,
                    span: type_decl.name_span,
                    children,
                },
            ));
        }
        written.sort_by_key(|(start, _)| *start);
        written.into_iter().map(|(_, outline)| outline).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{VirtualSource, build};
    use std::path::PathBuf;

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

    /// The outline of the entry file, as (kind, text) pairs.
    fn outline(files: &[(&str, &str)]) -> Vec<(OutlineKind, String)> {
        let source = files[0].1;
        let built = built(files);
        built
            .outline(&entry(&built))
            .into_iter()
            .map(|symbol| {
                (
                    symbol.kind,
                    source[symbol.span.start..symbol.span.end].to_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn a_file_outlines_its_declarations_and_type_members() {
        let files = [(
            "main.kiru",
            "rec Point { x: int, y: int };\nenum Shape { Dot, Ring(int) };\nfn area(shape: Shape) -> int { return 0; };\nfn main() { return; };",
        )];
        let outline = outline(&files);
        assert_eq!(
            outline
                .iter()
                .map(|(kind, name)| (format!("{kind:?}"), name.clone()))
                .collect::<Vec<_>>(),
            vec![
                ("Rec".to_owned(), "Point".to_owned()),
                ("Enum".to_owned(), "Shape".to_owned()),
                ("Function".to_owned(), "area".to_owned()),
                ("Function".to_owned(), "main".to_owned()),
            ],
            "the outline carries every declaration in written order"
        );
        let built = built(&files);
        let point = built
            .outline(&entry(&built))
            .into_iter()
            .find(|symbol| symbol.name == "Point")
            .expect("the rec is outlined");
        assert_eq!(
            point
                .children
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            vec!["x", "y"],
            "a rec's fields are its children"
        );
        let shape = built
            .outline(&entry(&built))
            .into_iter()
            .find(|symbol| symbol.name == "Shape")
            .expect("the enum is outlined");
        assert_eq!(
            shape
                .children
                .iter()
                .map(|variant| variant.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Dot", "Ring"],
            "an enum's variants are its children"
        );
    }

    #[test]
    fn a_bound_function_is_marked_as_a_method() {
        let files = [(
            "main.kiru",
            "rec Point { x: int };\nimpl Point { pub fn at(self: Point, index: int) -> int { return 0; }; };\nfn main() { return; };",
        )];
        let outline = outline(&files);
        assert_eq!(
            outline
                .iter()
                .map(|(kind, name)| (format!("{kind:?}"), name.clone()))
                .collect::<Vec<_>>(),
            vec![
                ("Rec".to_owned(), "Point".to_owned()),
                ("Method".to_owned(), "at".to_owned()),
                ("Function".to_owned(), "main".to_owned()),
            ],
            "an impl member is filed under its type, so the outline marks it bound"
        );
    }
}
