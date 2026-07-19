//! # Bucket Registry (3-bucket namespace model)
//!
//! kiru is an IaC task runner, not a general-purpose language, and the runner
//! has no notion of runtime scope — `crate::plan` inlines every variable to a
//! `String` at compile time. Scoping therefore exists only to (1) detect
//! duplicate declarations and (2) resolve `$name` references at compile time.
//!
//! There are exactly three declaration buckets:
//!
//! - **global** — `var` declared at the top level (outside any `pr`).
//! - **project** — a `var` written in a `pr` body, a `fn` body, or an `env`
//!   block. All three collapse into one project bucket: a `fn`-body `var`
//!   becomes project-global, so two functions declaring `var x` now collide.
//! - **case** — a transient bucket that exists only while resolving a single
//!   `case` arm. It isolates arm-local `var` declarations from one another and
//!   shadows the project/global buckets for the duration of that arm.
//!
//! Resolution precedence for a `$name` reference is **case, then project, then
//! global**. Duplicates are an error *within* a bucket only; the same name may
//! legally appear in two different buckets (there is no ancestor-chain shadow
//! rule). See `plan.md` section 2 for the authoritative spec.

use std::collections::HashMap;
use std::fmt;

/// Identifies which declaration bucket a name lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Global,
    Project,
    Case,
}

impl fmt::Display for Bucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Bucket::Global => write!(f, "top level"),
            Bucket::Project => write!(f, "project"),
            Bucket::Case => write!(f, "this case arm"),
        }
    }
}

/// Error returned when a variable is redeclared within the same bucket.
#[derive(Debug, Clone)]
pub struct Redeclaration {
    pub name: String,
    pub existing: Bucket,
}

/// The 3-bucket namespace registry. Generic over the value type `V`:
/// resolution uses `BucketRegistry<String>` (inlined values), validation uses
/// `BucketRegistry<()>` (presence only).
#[derive(Debug, Clone)]
pub struct BucketRegistry<V> {
    global: HashMap<String, V>,
    project: HashMap<String, V>,
    /// Transient per case-arm bucket. `None` means we are not currently
    /// resolving a case arm.
    case: Option<HashMap<String, V>>,
}

impl<V> BucketRegistry<V> {
    /// Create an empty registry: empty global and project buckets, no case
    /// bucket active.
    pub fn new() -> Self {
        BucketRegistry {
            global: HashMap::new(),
            project: HashMap::new(),
            case: None,
        }
    }

    /// Seed the global bucket with pre-validated entries (no duplicate check).
    /// Intended for hydrating the registry at a phase boundary where the data
    /// is already known to be conflict-free.
    pub fn seed_global(&mut self, entries: impl IntoIterator<Item = (String, V)>) {
        self.global.extend(entries);
    }

    /// Seed the project bucket with pre-validated entries (no duplicate check).
    pub fn seed_project(&mut self, entries: impl IntoIterator<Item = (String, V)>) {
        self.project.extend(entries);
    }

    /// Iterate over the global bucket's entries.
    pub fn iter_global(&self) -> impl Iterator<Item = (&String, &V)> {
        self.global.iter()
    }

    /// Open a fresh, empty case bucket. Returns a guard that closes the bucket
    /// (restores `None`) when dropped, so sibling arms are automatically
    /// independent.
    pub fn enter_case(&mut self) -> CaseGuard<'_, V> {
        self.case = Some(HashMap::new());
        CaseGuard { reg: self }
    }

    /// Look up a name using precedence case, then project, then global.
    pub fn lookup(&self, name: &str) -> Option<&V> {
        self.case
            .as_ref()
            .and_then(|case| case.get(name))
            .or_else(|| self.project.get(name))
            .or_else(|| self.global.get(name))
    }

    /// Whether a name is visible in any bucket (case, then project, then
    /// global). Used for undefined-variable diagnostics during validation.
    pub fn is_declared(&self, name: &str) -> bool {
        self.lookup(name).is_some()
    }

    /// Insert `name -> value` into `bucket`, erroring if `name` is already
    /// present. Shared by every `declare_*` method so the duplicate-detection
    /// rule lives in exactly one place.
    fn insert_unique(
        bucket: &mut HashMap<String, V>,
        name: String,
        value: V,
        existing: Bucket,
    ) -> Result<(), Redeclaration> {
        if bucket.contains_key(&name) {
            return Err(Redeclaration { name, existing });
        }
        bucket.insert(name, value);
        Ok(())
    }

    /// Declare a top-level variable into the global bucket.
    pub fn declare_global(&mut self, name: String, value: V) -> Result<(), Redeclaration> {
        Self::insert_unique(&mut self.global, name, value, Bucket::Global)
    }

    /// Declare a project-scoped variable into the project bucket (pr-body,
    /// fn-body, or env `var`).
    pub fn declare_project(&mut self, name: String, value: V) -> Result<(), Redeclaration> {
        Self::insert_unique(&mut self.project, name, value, Bucket::Project)
    }

    /// Declare a variable into the active bucket for a normal (non-top-level)
    /// declaration: the case bucket when inside a `case` arm, otherwise the
    /// project bucket. This is the single entry point for `var` declarations
    /// that appear in a `pr`/`fn`/`env`/`case` context.
    pub fn declare_scoped(&mut self, name: String, value: V) -> Result<(), Redeclaration> {
        if self.case.is_some() {
            self.declare_case(name, value)
        } else {
            self.declare_project(name, value)
        }
    }

    /// Declare an arm-local variable into the active case bucket. Must only be
    /// called while a case bucket is open (see [`enter_case`]).
    pub fn declare_case(&mut self, name: String, value: V) -> Result<(), Redeclaration> {
        let case = self
            .case
            .as_mut()
            .expect("declare_case called outside of a case arm");
        Self::insert_unique(case, name, value, Bucket::Case)
    }

    /// Replace the value of an already-declared variable, searching the case
    /// bucket, then the project bucket, then the global bucket, and updating
    /// the first match. Used by the config-eval phase to swap a `var shell`
    /// placeholder for its real (shell-evaluated) output. If the name is
    /// absent, it is inserted into the global bucket.
    pub fn update(&mut self, name: &str, value: V) {
        if self.case.as_ref().is_some_and(|c| c.contains_key(name)) {
            self.case.as_mut().unwrap().insert(name.to_string(), value);
            return;
        }
        if self.project.contains_key(name) {
            self.project.insert(name.to_string(), value);
            return;
        }
        self.global.insert(name.to_string(), value);
    }
}

impl<V> Default for BucketRegistry<V> {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard returned by [`BucketRegistry::enter_case`]. Restores `case` to
/// `None` on drop, isolating sibling arms.
pub struct CaseGuard<'a, V> {
    reg: &'a mut BucketRegistry<V>,
}

impl<V> Drop for CaseGuard<'_, V> {
    fn drop(&mut self) {
        self.reg.case = None;
    }
}

impl<'a, V> CaseGuard<'a, V> {
    /// Borrow the underlying registry while the case bucket is active.
    pub fn scope(&mut self) -> &mut BucketRegistry<V> {
        self.reg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_bucket_isolated() {
        let mut reg: BucketRegistry<String> = BucketRegistry::new();
        reg.declare_global("x".to_string(), "g".to_string())
            .unwrap();
        assert_eq!(reg.lookup("x"), Some(&"g".to_string()));
    }

    #[test]
    fn test_cross_bucket_same_name_allowed() {
        let mut reg: BucketRegistry<String> = BucketRegistry::new();
        reg.declare_global("name".to_string(), "global".to_string())
            .unwrap();
        // A project-scoped `name` does NOT collide with the global `name`.
        reg.declare_project("name".to_string(), "project".to_string())
            .unwrap();
        assert_eq!(reg.lookup("name"), Some(&"project".to_string()));
    }

    #[test]
    fn test_within_bucket_duplicate_errors() {
        let mut reg: BucketRegistry<String> = BucketRegistry::new();
        reg.declare_project("x".to_string(), "a".to_string())
            .unwrap();
        let err = reg
            .declare_project("x".to_string(), "b".to_string())
            .unwrap_err();
        assert_eq!(err.name, "x");
        assert_eq!(err.existing, Bucket::Project);
    }

    #[test]
    fn test_case_bucket_shadows_and_isolates() {
        let mut reg: BucketRegistry<String> = BucketRegistry::new();
        reg.declare_project("x".to_string(), "proj".to_string())
            .unwrap();
        {
            let guard = reg.enter_case();
            guard
                .reg
                .declare_case("x".to_string(), "arm".to_string())
                .unwrap();
            // within the arm, the case bucket wins
            assert_eq!(guard.reg.lookup("x"), Some(&"arm".to_string()));
            // a second declaration in the same arm collides
            assert!(
                guard
                    .reg
                    .declare_case("x".to_string(), "again".to_string())
                    .is_err()
            );
        }
        // after the arm, the case bucket is gone and the project value shows
        assert_eq!(reg.lookup("x"), Some(&"proj".to_string()));
    }

    #[test]
    fn test_sibling_case_arms_independent() {
        let mut reg: BucketRegistry<String> = BucketRegistry::new();
        {
            let guard = reg.enter_case();
            guard
                .reg
                .declare_case("x".to_string(), "a".to_string())
                .unwrap();
        }
        {
            let guard = reg.enter_case();
            // fresh bucket — no collision with the previous arm
            assert!(
                guard
                    .reg
                    .declare_case("x".to_string(), "b".to_string())
                    .is_ok()
            );
        }
    }

    // ── Compiler-level integration tests for bucket semantics ──────────

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
    fn test_project_var_and_global_same_name_no_error() {
        // Cross-bucket reuse of a name is allowed: a top-level `name` and a
        // project-body `name` live in different buckets.
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
        compile_full(&dir.path().join("main.kiru")).unwrap();
    }

    #[test]
    fn test_fn_body_var_and_pr_body_var_collide() {
        // fn-body `var` lands in the project bucket, so it collides with a
        // pr-body `var` of the same name (both are project-scoped).
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
    fn test_sibling_fns_same_var_name_collide() {
        // fn-body `var` is project-global, so two functions declaring `var x`
        // collide in the project bucket.
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
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("$x is already defined"),
            "got: {}",
            err
        );
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
    fn test_fn_var_then_case_var_no_error() {
        // A fn-body `var` (project bucket) and an arm-local `var` (case bucket)
        // are in different buckets and may share a name.
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
        compile_full(&dir.path().join("main.kiru")).unwrap();
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
