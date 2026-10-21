//! The semantic tokens a file earns: the model's roles laid over the file's
//! tokens.
//!
//! One legend serves every file, declared here once, in the order the roles
//! are enumerated. A token's role is a fact the model already holds — the
//! declaration a name stands on, the read it makes, the write it performs,
//! the member it accesses — and a node the model carries no role fact for
//! emits no token: the highlighting is what the model checked, not a second
//! reading of the surface. The tokens the lexer writes beside the model's
//! names — the keywords, the literals, the operators, the comments — are the
//! lexer's own facts, carried as they were skipped.

use std::collections::HashSet;
use std::path::Path;

use crate::compiler::build::Build;
use crate::compiler::model::{
    DeclarationId, DeclarationKind, FileId, TypeKind, UseTarget, scalar_from_name,
};
use crate::compiler::reads::{Reads, collect_reads};
use crate::syntax::{Span, Token, TokenKind, lex_with_comments};

/// The legend's token types, in the order the roles enumerate them.
pub const TOKEN_TYPES: &[&str] = &[
    "namespace",
    "type",
    "function",
    "method",
    "property",
    "enumMember",
    "variable",
    "parameter",
    "keyword",
    "string",
    "number",
    "operator",
    "comment",
];

/// The legend's token modifiers, in the order the bitset sets them.
pub const TOKEN_MODIFIERS: &[&str] = &["declaration", "definition"];

/// A role the model marks a written name with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticRole {
    /// A name a path descends through.
    Namespace,
    /// A name that stands for a type.
    Type,
    /// A namespace-level function or native.
    Function,
    /// A function filed under a type.
    Method,
    /// A rec field's declaration or access.
    Property,
    /// An enum variant's declaration.
    EnumMember,
    /// A module value or a local binding.
    Variable,
    /// A parameter.
    Parameter,
    /// A word the lexer writes for a form, and the wildcard's.
    Keyword,
    /// A string literal.
    Text,
    /// An integer literal.
    Number,
    /// An operator or a punctuation token.
    Operator,
    /// A comment the tokens skipped.
    Comment,
}

impl SemanticRole {
    /// The role's place in the legend, which its enumeration declares.
    pub fn slot(self) -> usize {
        self as usize
    }
}

/// One token the file earns: the span, the role the model marks it with, and
/// whether the span is a declaration's own name.
#[derive(Debug)]
pub struct SemanticToken {
    pub span: Span,
    pub role: SemanticRole,
    pub definition: bool,
}

/// One fact a file's names carry: the span, the role, and whether the span
/// is a declaration's own name.
struct Fact {
    span: Span,
    role: SemanticRole,
    definition: bool,
}

/// The facts one file's names carry, beside the written paths a read spells.
struct Facts {
    facts: Vec<Fact>,
    paths: Vec<Span>,
}

impl Build {
    /// The semantic tokens of the file `path` holds, in the order the spans
    /// stand in the source. A path the program reads no file by earns none.
    pub fn semantic_tokens(&self, path: &Path) -> Vec<SemanticToken> {
        let Some(file) = self.program.files.iter().position(|file| file.path == path) else {
            return Vec::new();
        };
        let file = FileId(file);
        let source = self.loaded.files[file.0].source.as_str();
        let Ok((tokens, comments)) = lex_with_comments(source) else {
            return Vec::new();
        };
        let facts = self.facts(file, &collect_reads(&self.program));
        let mut earned: Vec<SemanticToken> = comments
            .into_iter()
            .map(|span| SemanticToken {
                span,
                role: SemanticRole::Comment,
                definition: false,
            })
            .collect();
        for token in tokens {
            if let Some((role, definition)) = role_of(
                &token,
                source,
                facts.facts.as_slice(),
                facts.paths.as_slice(),
            ) {
                earned.push(SemanticToken {
                    span: token.span,
                    role,
                    definition,
                });
            }
        }
        earned.sort_by_key(|token| token.span.start);
        earned
    }

    /// The facts the file's written names carry: the declarations' own
    /// names, the reads and writes the bodies perform, and the members the
    /// declared types declare.
    fn facts(&self, file: FileId, reads: &Reads) -> Facts {
        let parameters: HashSet<DeclarationId> = self
            .program
            .declarations
            .iter()
            .filter_map(|declaration| declaration.function())
            .flat_map(|function| function.parameters.iter().copied())
            .collect();
        let mut facts = Vec::new();
        let mut paths = Vec::new();
        // The bindings the bodies declare: a parameter and a local, whose
        // name spans are the names the sources wrote.
        for bound in reads.bindings.iter().filter(|bound| bound.file == file) {
            let declaration = self.program.declaration(bound.declaration);
            let role = if parameters.contains(&bound.declaration) {
                SemanticRole::Parameter
            } else {
                SemanticRole::Variable
            };
            facts.push(Fact {
                span: declaration.name_span,
                role,
                definition: true,
            });
        }
        // The declarations the file introduced: its namespace-level names
        // and the functions its impl blocks filed.
        for id in &self.program.files[file.0].declarations {
            let declaration = self.program.declaration(*id);
            let role = match declaration.kind {
                DeclarationKind::Function | DeclarationKind::Native => SemanticRole::Function,
                // A member an impl block filed under its type.
                DeclarationKind::Impl => SemanticRole::Method,
                DeclarationKind::ModuleValue => SemanticRole::Variable,
                // A named type's name comes from the types table below.
                DeclarationKind::Rec | DeclarationKind::Enum => continue,
                DeclarationKind::Local => {
                    unreachable!("a file's declarations introduce written names")
                }
            };
            facts.push(Fact {
                span: declaration.name_span,
                role,
                definition: true,
            });
        }
        // The declared types of the file, and the members their bodies
        // declare.
        for type_decl in self
            .program
            .types
            .iter()
            .filter(|type_decl| type_decl.file == Some(file))
        {
            facts.push(Fact {
                span: type_decl.name_span,
                role: SemanticRole::Type,
                definition: true,
            });
            match &type_decl.kind {
                TypeKind::Rec(fields) => {
                    for field in fields {
                        facts.push(Fact {
                            span: field.name_span,
                            role: SemanticRole::Property,
                            definition: true,
                        });
                    }
                }
                TypeKind::Enum(variants) => {
                    for variant in variants {
                        facts.push(Fact {
                            span: variant.name_span,
                            role: SemanticRole::EnumMember,
                            definition: true,
                        });
                    }
                }
            }
        }
        // The reads the bodies perform, each by the name its path ends at,
        // and the whole path the descent names run through.
        for read in reads.reads.iter().filter(|read| read.file == file) {
            let source = &self.loaded.files[read.file.0].source;
            facts.push(Fact {
                span: read.name_span(source),
                role: self.role_of(read.declaration, &parameters),
                definition: false,
            });
            paths.push(read.span);
        }
        // The written type names the declarations and bodies spell: the name a
        // path ends at is the type, and the names it descends through are
        // namespaces.
        for used in self.type_uses.iter().filter(|used| used.file == file) {
            facts.push(Fact {
                span: used.name,
                role: SemanticRole::Type,
                definition: false,
            });
            paths.push(used.path);
        }
        // The writes, which name their binding without reading it.
        for write in reads.writes.iter().filter(|write| write.file == file) {
            facts.push(Fact {
                span: write.span,
                role: SemanticRole::Variable,
                definition: false,
            });
        }
        // The members the bodies access.
        for property in reads
            .properties
            .iter()
            .filter(|property| property.file == file)
        {
            facts.push(Fact {
                span: property.span,
                role: SemanticRole::Property,
                definition: false,
            });
        }
        // The names the file's `use`s deliver, each the one the path ends at.
        let deliveries = self.deliveries();
        for (use_index, used) in self.loaded.uses.iter().enumerate() {
            if used.file != file.0 {
                continue;
            }
            let role = match deliveries.get(&use_index) {
                Some(UseTarget::Function(declaration)) => self.role_of(*declaration, &parameters),
                Some(UseTarget::Type(_)) => SemanticRole::Type,
                Some(UseTarget::Namespace(_)) => SemanticRole::Namespace,
                None => continue,
            };
            facts.push(Fact {
                span: used.delivered,
                role,
                definition: false,
            });
        }
        Facts { facts, paths }
    }

    /// The role a written name earns by naming `target`.
    fn role_of(&self, target: DeclarationId, parameters: &HashSet<DeclarationId>) -> SemanticRole {
        let declaration = self.program.declaration(target);
        match declaration.kind {
            DeclarationKind::Function | DeclarationKind::Native => SemanticRole::Function,
            // A member an impl block filed under its type.
            DeclarationKind::Impl => SemanticRole::Method,
            DeclarationKind::ModuleValue => SemanticRole::Variable,
            DeclarationKind::Local => {
                if parameters.contains(&target) {
                    SemanticRole::Parameter
                } else {
                    SemanticRole::Variable
                }
            }
            // A type name is written where a declared type is read.
            DeclarationKind::Rec | DeclarationKind::Enum => SemanticRole::Type,
        }
    }
}

/// The role a token earns: a name reads its fact when the model holds one,
/// else the descent it is written in names a namespace, else the word the
/// scalar table declares is a written type name, and a name with no fact
/// earns nothing. The lexer's own words read their own kinds.
fn role_of(
    token: &Token,
    source: &str,
    facts: &[Fact],
    paths: &[Span],
) -> Option<(SemanticRole, bool)> {
    match &token.kind {
        TokenKind::Ident => {
            let covered = facts.iter().find(|fact| {
                fact.span.contains(token.span.start) && token.span.end <= fact.span.end
            });
            if let Some(fact) = covered {
                return Some((fact.role, fact.definition));
            }
            let descended = paths
                .iter()
                .any(|path| path.contains(token.span.start) && token.span.end <= path.end);
            if descended {
                return Some((SemanticRole::Namespace, false));
            }
            // A word the model's scalar table declares a type by is written
            // type: the fact is the table's own.
            let word = &source[token.span.start..token.span.end];
            scalar_from_name(word).map(|_| (SemanticRole::Type, false))
        }
        TokenKind::Wildcard
        | TokenKind::Use
        | TokenKind::Pub
        | TokenKind::Fn
        | TokenKind::Impl
        | TokenKind::Enum
        | TokenKind::Let
        | TokenKind::Rec
        | TokenKind::Match
        | TokenKind::Return
        | TokenKind::Panic
        | TokenKind::Async
        | TokenKind::Wait
        | TokenKind::For
        | TokenKind::Break
        | TokenKind::True
        | TokenKind::False => Some((SemanticRole::Keyword, false)),
        TokenKind::Text(_) => Some((SemanticRole::Text, false)),
        TokenKind::Int(_) => Some((SemanticRole::Number, false)),
        TokenKind::PathSep
        | TokenKind::Dot
        | TokenKind::Colon
        | TokenKind::Comma
        | TokenKind::Semi
        | TokenKind::Equals
        | TokenKind::EqEq
        | TokenKind::NotEq
        | TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Slash
        | TokenKind::Percent
        | TokenKind::Bang
        | TokenKind::AndAnd
        | TokenKind::OrOr
        | TokenKind::Arrow
        | TokenKind::FatArrow
        | TokenKind::Less
        | TokenKind::LessEq
        | TokenKind::Greater
        | TokenKind::GreaterEq
        | TokenKind::LBrace
        | TokenKind::RBrace
        | TokenKind::LParen
        | TokenKind::RParen
        | TokenKind::InterpolationStart
        | TokenKind::InterpolationEnd => Some((SemanticRole::Operator, false)),
        TokenKind::Eof => None,
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

    /// The tokens of the entry file, as (legend kind, text) pairs.
    fn tokens(built: &Build, source: &str) -> Vec<(String, String)> {
        built
            .semantic_tokens(&entry(built))
            .into_iter()
            .map(|token| {
                (
                    format!(
                        "{}{}",
                        TOKEN_TYPES[token.role.slot()],
                        if token.definition { "*" } else { "" }
                    ),
                    source[token.span.start..token.span.end].to_owned(),
                )
            })
            .collect()
    }

    /// The roles the word `text` earned, painted.
    fn roles_of(painted: &[(String, String)], text: &str) -> Vec<String> {
        painted
            .iter()
            .filter(|(_, token_text)| token_text == text)
            .map(|(kind, _)| kind.clone())
            .collect()
    }

    const PROGRAM: &str = "use root::speak;\nrec Point { x: int, y: int };\nenum Shape { Dot };\nimpl Point {\n  pub fn at(self: Point, index: int) -> int { return index; };\n};\nfn speak() -> txt { return \"spoken\"; };\nfn main() {\n  let point: Point = { x = 1, y = 2 };\n  let count: int = point.at(0);\n  point.x = count;\n  std::print(speak());\n};";

    #[test]
    fn the_legend_enumerates_the_roles_it_names() {
        let roles = [
            SemanticRole::Namespace,
            SemanticRole::Type,
            SemanticRole::Function,
            SemanticRole::Method,
            SemanticRole::Property,
            SemanticRole::EnumMember,
            SemanticRole::Variable,
            SemanticRole::Parameter,
            SemanticRole::Keyword,
            SemanticRole::Text,
            SemanticRole::Number,
            SemanticRole::Operator,
            SemanticRole::Comment,
        ];
        let legend = [
            "namespace",
            "type",
            "function",
            "method",
            "property",
            "enumMember",
            "variable",
            "parameter",
            "keyword",
            "string",
            "number",
            "operator",
            "comment",
        ];
        for (slot, role) in roles.iter().enumerate() {
            assert_eq!(role.slot(), slot, "every role enumerates in legend order");
            assert_eq!(
                TOKEN_TYPES[slot], legend[slot],
                "the legend's protocol spelling"
            );
        }
        assert_eq!(TOKEN_TYPES.len(), 13, "the legend declares every role once");
        assert_eq!(TOKEN_MODIFIERS, ["declaration", "definition"]);
    }

    #[test]
    fn a_checked_file_paints_the_model_s_roles() {
        let program = built(&[("main.kiru", PROGRAM)]);
        let painted = tokens(&program, PROGRAM);
        assert_eq!(
            roles_of(&painted, "Point"),
            vec!["type*", "type", "type"],
            "a declared type's name, and the three written annotations"
        );
        assert_eq!(
            roles_of(&painted, "x"),
            vec!["property*", "property"],
            "a rec field's own name and its access"
        );
        assert_eq!(
            roles_of(&painted, "Dot"),
            vec!["enumMember*"],
            "a variant's own name"
        );
        assert_eq!(
            roles_of(&painted, "at"),
            vec!["method*", "method"],
            "an impl member and its call"
        );
        assert_eq!(
            roles_of(&painted, "self"),
            vec!["parameter*"],
            "the receiver parameter"
        );
        assert_eq!(
            roles_of(&painted, "index"),
            vec!["parameter*", "parameter"],
            "a parameter and its read"
        );
        assert_eq!(
            roles_of(&painted, "point"),
            vec!["variable*", "variable", "variable"],
            "a binding, its read, and its write"
        );
        assert_eq!(
            roles_of(&painted, "count"),
            vec!["variable*", "variable"],
            "a binding and its read"
        );
        assert_eq!(
            roles_of(&painted, "speak"),
            vec!["function", "function*", "function"],
            "a use-delivered function: the use item's name, the declaration, and the read"
        );
        assert_eq!(
            roles_of(&painted, "print"),
            vec!["function"],
            "a native read"
        );
        assert_eq!(
            roles_of(&painted, "std"),
            vec!["namespace"],
            "a path's descent names"
        );
        assert_eq!(
            roles_of(&painted, "txt"),
            vec!["type"],
            "a written scalar type"
        );
        assert_eq!(
            roles_of(&painted, "fn"),
            vec!["keyword", "keyword", "keyword"],
            "the lexer's own words"
        );
        assert_eq!(
            roles_of(&painted, "\"spoken\""),
            vec!["string"],
            "a string literal"
        );
        assert_eq!(
            roles_of(&painted, "0"),
            vec!["number"],
            "an integer literal"
        );
        assert_eq!(
            roles_of(&painted, "use"),
            vec!["keyword"],
            "the use keyword paints"
        );
    }

    #[test]
    fn a_comment_paints_itself_and_a_foreign_file_paints_nothing() {
        let files = [("main.kiru", "fn main() { return; }; # done")];
        let program = built(&files);
        assert_eq!(
            tokens(&program, files[0].1).last(),
            Some(&("comment".to_owned(), "# done".to_owned())),
            "the comment the tokens skip paints its span"
        );
        assert!(
            program
                .semantic_tokens(Path::new("nowhere.kiru"))
                .is_empty(),
            "a file the program reads by no path earns no tokens"
        );
    }
}
