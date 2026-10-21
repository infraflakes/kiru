//! The compiler's error strings.
//!
//! Every message a stage reports is built here, so one wording serves every
//! site that raises the same error.

/// A count and its noun, pluralized: `1 argument`, `2 arguments`. Every arity
/// diagnostic reads through this, so the singular is never wrong.
pub(crate) fn count(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// The message for a variant payload that does not carry one item per declared
/// payload type, or `None` when it does. `subject` is how the site writes the
/// variant and `noun` names its items there, so a construction and a pattern
/// share the count check while each keeps its own noun.
pub(crate) fn variant_payload_count(
    subject: &str,
    declared: usize,
    found: usize,
    noun: &str,
) -> Option<String> {
    (found != declared)
        .then(|| format!("`{subject}` takes {}, found {found}", count(declared, noun)))
}

/// The message for a value of the wrong type: `expected <required>, found
/// <actual>`, where both names come from the type system.
pub(crate) fn expected_instead(required: &str, actual: &str) -> String {
    format!("expected {required}, found {actual}")
}

/// The message for a native named where a value is required. A native has no
/// runtime value yet: a function value is a code pointer, and a native is
/// dispatched by index.
pub(crate) fn native_used_as_value(name: &str) -> String {
    format!("`{name}` is a native and has no value form; call it")
}

/// The message for a value called as a function.
pub(crate) fn value_called(name: &str) -> String {
    format!("`{name}` is a value and cannot be called")
}

/// The message for a name declared more than once in one scope.
pub(crate) fn duplicate_name(name: &str) -> String {
    format!("`{name}` is declared more than once")
}

/// The message for a record literal that repeats a field name.
pub(crate) fn duplicate_field(name: &str) -> String {
    format!("duplicate field `{name}`")
}

/// The message for a name declared more than once in one namespace.
pub(crate) fn duplicate_in_namespace(name: &str) -> String {
    format!("`{name}` is declared more than once in this namespace")
}

/// The message a duplicate points back with: the fact that the place it
/// names is the first declaration of the name.
pub(crate) fn first_declaration(name: &str) -> String {
    format!("first declaration of `{name}`")
}

/// The message for a field or variant reached across a namespace that does not
/// mark it `pub`. A member is private to its own namespace, exactly like a
/// declaration.
pub(crate) fn private_member(owner: &str, member: &str, namespace: &str) -> String {
    format!("`{owner}.{member}` is private to `{namespace}`; mark it `pub` to reach it")
}

/// The message for a word that names no declaration: a rename's target name,
/// checked against the lexer's own rule for a name.
pub(crate) fn invalid_name(name: &str) -> String {
    format!("`{name}` is not a name")
}

/// The message for a declaration written in no file of the workspace: the
/// runtime's own natives, and the standard library's declarations, which the
/// program embeds rather than reads.
pub(crate) fn no_workspace_file(name: &str) -> String {
    format!("`{name}` is written in no file of the workspace")
}
