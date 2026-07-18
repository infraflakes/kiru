use crate::plan::{Plan, PlanProject, PlanStmt};
use crate::runner::OutputCallback;
use crate::runner::error::RuntimeError;
use crate::runner::execution_context::ExecContext;
use std::sync::Arc;

/// Executes resolved function bodies against a compiled `Plan`.
pub(crate) struct Runner {
    cfg: Arc<Plan>,
    output: OutputCallback,
}

/// Look up a function body by name inside a resolved project.
///
/// Centralizes the function lookup plus its `unknown function` error so the CLI
/// entry point and the runner never diverge on how a missing function is
/// reported.
pub(crate) fn resolve_project_fn<'a>(
    project: &'a PlanProject,
    project_name: &str,
    fn_name: &str,
) -> Result<&'a [PlanStmt], RuntimeError> {
    project
        .functions
        .get(fn_name)
        .map(Vec::as_slice)
        .ok_or_else(|| {
            RuntimeError::Lookup(format!(
                "unknown function {} in project {}",
                fn_name, project_name
            ))
        })
}

impl Runner {
    /// Create a runner that forwards every emitted output line to `output`.
    pub(crate) fn new(cfg: Arc<Plan>, output: OutputCallback) -> Self {
        Runner { cfg, output }
    }

    /// Look up and execute a function within a named project.
    pub(crate) fn execute_fn_call(
        &mut self,
        fn_name: &str,
        project_name: &str,
    ) -> Result<(), RuntimeError> {
        let project =
            self.cfg.projects.get(project_name).ok_or_else(|| {
                RuntimeError::Lookup(format!("unknown project: {}", project_name))
            })?;

        let fn_body = resolve_project_fn(project, project_name, fn_name)?;

        let mut ctx = ExecContext::new(Some(project), &mut self.output);
        ctx.exec_stmts(fn_body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::test_support::*;

    #[test]
    fn test_case_runtime_matching_arm() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `http://example.com`\n\
            dir = `test`\n\
        ] {\n\
            var string os = `Linux`;\n\
            fn deploy {\n\
                case $os {\n\
                    `Linux` { log `matched`; };\n\
                    _ { log `default`; };\n\
                };\n\
            }\n\
        }\
        ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let mut runner = Runner::new(Arc::new(cfg), Arc::new(|_| {}));
        runner.execute_fn_call("deploy", "test").unwrap();
    }

    #[test]
    fn test_case_runtime_no_match() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `http://example.com`\n\
            dir = `test`\n\
        ] {\n\
            var string os = `Darwin`;\n\
            fn deploy {\n\
                case $os {\n\
                    `Linux` { log `only-linux`; };\n\
                };\n\
            }\n\
        }\
        ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let mut runner = Runner::new(Arc::new(cfg), Arc::new(|_| {}));
        runner.execute_fn_call("deploy", "test").unwrap();
    }
}
