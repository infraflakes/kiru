use super::context::ExecContext;
use crate::dsl::ast::Expr;
use crate::runner::error::RuntimeError;

impl<'a> ExecContext<'a> {
    pub(super) fn resolve_expr(&self, expr: &Expr) -> Result<String, RuntimeError> {
        expr.resolve(&self.vars).map_err(RuntimeError::new)
    }

    pub(super) fn build_env(&self) -> Vec<(String, String)> {
        let mut env: std::collections::HashMap<String, String> =
            self.sys_env.iter().cloned().collect();

        for layer in &self.env_stack {
            for (key, value) in layer {
                env.insert(key.clone(), value.clone());
            }
        }

        env.into_iter().collect()
    }
}
