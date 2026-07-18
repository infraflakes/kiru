//! # Scope Stack
//!
//! Ordinary lexical scoping with exactly one deviation: declaring a name is
//! an error when that name is already visible from any enclosing scope (no
//! shadowing along the ancestor chain).  `env` blocks intentionally do NOT
//! open a scope frame — a `var` inside `env {}` lands in the enclosing
//! `fn`/`case` frame.
//!
//! - `lookup(name)` walks the frame stack innermost-first over the whole chain.
//! - `declare(name, value)` errors if `lookup(name)` succeeds (i.e. the name
//!   is visible from the current scope or any enclosing scope); otherwise it
//!   inserts into the top frame.
//! - `enter(kind)` pushes a fresh frame and returns a RAII guard that pops it
//!   on drop, making sibling scopes independent automatically.
//!
//! The Global frame is stored as a separate field so the top frame is always
//! reachable without `unwrap`/`expect`.

use std::collections::HashSet;
use std::fmt;

/// Identifies which kind of scope a frame represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Global,
    Project,
    Function,
    Case,
}

impl fmt::Display for ScopeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScopeKind::Global => write!(f, "top level"),
            ScopeKind::Project => write!(f, "project"),
            ScopeKind::Function => write!(f, "this function"),
            ScopeKind::Case => write!(f, "this case arm"),
        }
    }
}

/// A single frame in the scope stack, tagged with its kind.
#[derive(Debug, Clone)]
struct Frame<V> {
    kind: ScopeKind,
    entries: Vec<(String, V)>,
    /// Names that have been reserved (e.g. non-field-referenced project vars
    /// during linear processing) but not yet given a real value.  These are
    /// checked for duplicate detection but never returned by [`lookup`].
    reserved: HashSet<String>,
}

/// Error returned when a variable is redeclared.
#[derive(Debug, Clone)]
pub struct Redeclaration {
    pub name: String,
    pub existing_kind: ScopeKind,
}

/// A stack of lexical scope frames.  The Global frame is always stored
/// as a separate field so the top frame is provably reachable without
/// `unwrap`/`expect` — pushed frames live in `frames`.
///
/// Generic over the value type `V` — resolution uses `ScopeStack<String>`,
/// validation uses `ScopeStack<()>`.
#[derive(Debug, Clone)]
pub struct ScopeStack<V> {
    global: Frame<V>,
    frames: Vec<Frame<V>>,
}

impl<V> ScopeStack<V> {
    /// Create a new scope stack with an empty Global frame.
    pub fn new() -> Self {
        ScopeStack {
            global: Frame {
                kind: ScopeKind::Global,
                entries: Vec::new(),
                reserved: HashSet::new(),
            },
            frames: Vec::new(),
        }
    }

    /// Push a new scope frame of the given kind and return a guard that pops
    /// it on drop.
    pub fn enter(&mut self, kind: ScopeKind) -> ScopeGuard<'_, V> {
        self.frames.push(Frame {
            kind,
            entries: Vec::new(),
            reserved: HashSet::new(),
        });
        ScopeGuard { stack: self }
    }

    /// Push a permanent scope frame (not managed by a RAII guard).
    pub fn push_frame(&mut self, kind: ScopeKind) {
        self.frames.push(Frame {
            kind,
            entries: Vec::new(),
            reserved: HashSet::new(),
        });
    }

    /// Seed the global frame with pre-validated entries.
    ///
    /// Unlike [`declare`](Self::declare), this does NOT run the duplicate
    /// check — the caller guarantees the entries have already been validated.
    /// Intended for hydrating a scope stack at a phase boundary where the
    /// data is known to be conflict-free.
    pub fn seed_global(&mut self, entries: impl IntoIterator<Item = (String, V)>) {
        self.global.entries.extend(entries);
    }

    /// Seed the topmost frame with pre-validated entries (no duplicate check).
    /// If no frames have been pushed, seeds the Global frame.
    pub fn seed_top(&mut self, entries: impl IntoIterator<Item = (String, V)>) {
        self.top_mut().entries.extend(entries);
    }

    /// Iterate over the global frame's entries.
    pub fn iter_global(&self) -> impl Iterator<Item = (&String, &V)> {
        self.global.entries.iter().map(|(k, v)| (k, v))
    }

    // ── private chain-walk helpers ───────────────────────────────────

    /// Walk the frame chain (pushed frames then global) looking for `name`.
    /// Returns the value and the kind of the declaring frame if found.
    /// Only checks real entries — reserved names (with no value) are not
    /// returned.  Used by [`lookup`].
    fn find_entry(&self, name: &str) -> Option<(&V, ScopeKind)> {
        for frame in self.frames.iter().rev() {
            for (k, v) in &frame.entries {
                if k == name {
                    return Some((v, frame.kind));
                }
            }
        }
        for (k, v) in &self.global.entries {
            if k == name {
                return Some((v, self.global.kind));
            }
        }
        None
    }

    /// Check whether `name` exists anywhere in the visible frame chain,
    /// including reserved-but-unresolved names.  Returns the kind of the
    /// innermost declaring/reserving frame.
    fn name_exists(&self, name: &str) -> Option<ScopeKind> {
        for frame in self.frames.iter().rev() {
            for (k, _) in &frame.entries {
                if k == name {
                    return Some(frame.kind);
                }
            }
            if frame.reserved.contains(name) {
                return Some(frame.kind);
            }
        }
        for (k, _) in &self.global.entries {
            if k == name {
                return Some(self.global.kind);
            }
        }
        None
    }

    // ── public accessors ──────────────────────────────────────────────

    /// Look up a name walking innermost-first over the whole chain
    /// (pushed frames then global).  Reserved names are invisible to
    /// lookups because they carry no value.
    pub fn lookup(&self, name: &str) -> Option<&V> {
        self.find_entry(name).map(|(v, _)| v)
    }

    /// Check whether a name is declared or reserved in any visible scope.
    pub fn is_declared(&self, name: &str) -> bool {
        self.name_exists(name).is_some()
    }

    /// Find the [`ScopeKind`] of the (innermost) frame where `name` is
    /// declared or reserved.  Returns `None` if the name is not visible
    /// in any scope.
    pub fn declaring_kind(&self, name: &str) -> Option<ScopeKind> {
        self.name_exists(name)
    }

    /// Declare a variable in the top (innermost) frame.
    ///
    /// Returns `Err(Redeclaration)` if the name is already visible from the
    /// current scope or any enclosing scope (including reserved names).
    /// Otherwise inserts the value into the top frame and returns `Ok(())`.
    ///
    /// The frame chain is walked once to compute both presence and the
    /// declaring scope kind.
    pub fn declare(&mut self, name: String, value: V) -> Result<(), Redeclaration> {
        if let Some(kind) = self.name_exists(&name) {
            return Err(Redeclaration {
                name,
                existing_kind: kind,
            });
        }
        self.top_mut().entries.push((name, value));
        Ok(())
    }

    /// The topmost writable frame — either the last pushed frame or the
    /// Global frame if no frames have been pushed.  Always succeeds because
    /// the Global frame exists by construction.
    fn top_mut(&mut self) -> &mut Frame<V> {
        if let Some(frame) = self.frames.last_mut() {
            frame
        } else {
            &mut self.global
        }
    }

    /// Replace the value of an already-declared variable, searching the frame
    /// chain innermost-first (like [`lookup`]). Used by the config-eval phase
    /// to swap a `var shell` placeholder for its real (shell-evaluated) output
    /// without disturbing declaration order or re-triggering duplicate checks.
    pub fn update(&mut self, name: &str, value: V) {
        for frame in self.frames.iter_mut().rev() {
            if let Some(entry) = frame.entries.iter_mut().find(|(k, _)| k == name) {
                entry.1 = value;
                return;
            }
        }
        if let Some(entry) = self.global.entries.iter_mut().find(|(k, _)| k == name) {
            entry.1 = value;
        }
    }
}

impl<V> Default for ScopeStack<V> {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard that pops a scope frame when dropped.
///
/// Returned by [`ScopeStack::enter`].
#[derive(Debug)]
pub struct ScopeGuard<'a, V> {
    pub(crate) stack: &'a mut ScopeStack<V>,
}

impl<V> Drop for ScopeGuard<'_, V> {
    fn drop(&mut self) {
        self.stack.frames.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_frame_always_present() {
        let stack: ScopeStack<String> = ScopeStack::new();
        assert_eq!(stack.frames.len(), 0);
        assert_eq!(stack.global.entries.len(), 0);
    }

    #[test]
    fn test_lookup_and_declare_global() {
        let mut stack: ScopeStack<String> = ScopeStack::new();
        stack.declare("x".to_string(), "hello".to_string()).unwrap();
        assert_eq!(stack.lookup("x"), Some(&"hello".to_string()));
    }

    #[test]
    fn test_redeclare_because_shadowing_not_allowed() {
        let mut stack: ScopeStack<String> = ScopeStack::new();
        stack
            .declare("x".to_string(), "global".to_string())
            .unwrap();
        {
            let guard = stack.enter(ScopeKind::Function);
            let err = guard
                .stack
                .declare("x".to_string(), "local".to_string())
                .unwrap_err();
            assert_eq!(err.existing_kind, ScopeKind::Global);
        }
    }

    #[test]
    fn test_sibling_independence() {
        let mut stack: ScopeStack<String> = ScopeStack::new();
        {
            let g1 = stack.enter(ScopeKind::Function);
            g1.stack
                .declare("x".to_string(), "fn_a".to_string())
                .unwrap();
        }
        {
            let g2 = stack.enter(ScopeKind::Function);
            assert!(
                g2.stack
                    .declare("x".to_string(), "fn_b".to_string())
                    .is_ok()
            );
            assert_eq!(g2.stack.lookup("x"), Some(&"fn_b".to_string()));
        }
        assert!(!stack.is_declared("x"));
    }

    #[test]
    fn test_redeclaration_error() {
        let mut stack: ScopeStack<String> = ScopeStack::new();
        stack.declare("x".to_string(), "first".to_string()).unwrap();
        let err = stack
            .declare("x".to_string(), "second".to_string())
            .unwrap_err();
        assert_eq!(err.name, "x");
        assert_eq!(err.existing_kind, ScopeKind::Global);
    }

    #[test]
    fn test_inner_sees_outer() {
        let mut stack: ScopeStack<String> = ScopeStack::new();
        stack
            .declare("outer".to_string(), "val".to_string())
            .unwrap();
        let guard = stack.enter(ScopeKind::Function);
        assert_eq!(guard.stack.lookup("outer"), Some(&"val".to_string()));
    }

    #[test]
    fn test_empty_lookup() {
        let stack: ScopeStack<String> = ScopeStack::new();
        assert_eq!(stack.lookup("nonexistent"), None);
    }

    #[test]
    fn test_seed_global_and_top() {
        let mut stack: ScopeStack<String> = ScopeStack::new();
        stack.seed_global([("k1".into(), "v1".into())]);
        assert_eq!(stack.lookup("k1"), Some(&"v1".into()));
        stack.push_frame(ScopeKind::Project);
        stack.seed_top([("k2".into(), "v2".into())]);
        assert_eq!(stack.lookup("k2"), Some(&"v2".into()));
    }

    // ── Compiler-level integration tests for scope semantics ──────────

    use crate::compiler::test_support::*;

    #[test]
    fn test_duplicate_global_var() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        var string x = `a`;\n\
        var string x = `b`;\n\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("$x is already defined"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_duplicate_var_in_fn_body() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            fn bad {\n\
                var string x = `a`;\n\
                var string x = `b`;\n\
            }\n\
        }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("$x is already defined"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_duplicate_var_in_fn_body_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            fn bad {\n\
                var string x = `a`;\n\
                var string x = `b`;\n\
            }\n\
        }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("$x is already defined"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_project_var_sees_global() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        var string global_var = `global`;\n\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            fn f { log $global_var; }\n\
        }\
        ",
        );
        // global vars should be accessible inside project function bodies
        compile_full(&dir.path().join("main.kiru")).unwrap();
    }

    #[test]
    fn test_project_var_cannot_shadow_global() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        var string name = `global`;\n\
        pr test [\n\
            url = `http://example.com`\n\
            dir = $name\n\
        ] {\n\
            var string name = `project`;\n\
        }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string()
                .contains("$name is already defined at top level"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_sibling_fns_same_var_name_no_error() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            fn a { var string x = `a`; log $x; }\n\
            fn b { var string x = `b`; log $x; }\n\
        }\
        ",
        );
        compile_full(&dir.path().join("main.kiru")).unwrap();
    }

    #[test]
    fn test_different_projects_same_var_name_no_error() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr p1 [\n\
            url = `u1`\n\
            dir = `d1`\n\
        ] {\n\
            var string x = `from-p1`;\n\
        }\n\
        pr p2 [\n\
            url = `u2`\n\
            dir = `d2`\n\
        ] {\n\
            var string x = `from-p2`;\n\
        }\
        ",
        );
        compile_full(&dir.path().join("main.kiru")).unwrap();
    }

    #[test]
    fn test_sibling_case_arms_same_var_name_no_error() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            var string os = `Linux`;\n\
            fn deploy {\n\
                case $os {\n\
                    `Linux` { var string x = `matched`; log $x; };\n\
                    _ { var string x = `default`; log $x; };\n\
                };\n\
            }\n\
        }\
        ",
        );
        compile_full(&dir.path().join("main.kiru")).unwrap();
    }

    #[test]
    fn test_project_var_then_fn_var_shadow_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            var string x = `project`;\n\
            fn bad {\n\
                var string x = `fn`;\n\
            }\n\
        }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("$x is already defined"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_fn_var_then_case_var_shadow_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            var string os = `Linux`;\n\
            fn bad {\n\
                var string x = `fn`;\n\
                case $os {\n\
                    `Linux` { var string x = `arm`; };\n\
                    _ { };\n\
                };\n\
            }\n\
        }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("$x is already defined"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_env_var_participates_in_enclosing_fn() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            fn deploy {\n\
                env [MY_VAR = `hello`] {\n\
                    var string x = `inside-env`;\n\
                };\n\
                log $x;\n\
            }\n\
        }\
        ",
        );
        compile_full(&dir.path().join("main.kiru")).unwrap();
    }

    #[test]
    fn test_env_var_redeclare_in_enclosing_fn_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            fn deploy {\n\
                var string x = `a`;\n\
                env [MY_VAR = `hello`] {\n\
                    var string x = `b`;\n\
                };\n\
            }\n\
        }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("$x is already defined"),
            "got: {}",
            err
        );
    }
}
