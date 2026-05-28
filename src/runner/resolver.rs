use super::context::ExecContext;
use crate::dsl::ast::Expr;
use crate::runner::error::RuntimeError;

impl<'a> ExecContext<'a> {
    pub(super) fn resolve_expr(&self, expr: &Expr) -> Result<String, RuntimeError> {
        expr.resolve(&self.vars).map_err(RuntimeError::Lookup)
    }

    pub(super) fn build_env(&self) -> impl Iterator<Item = (String, String)> + '_ {
        let sys = self.sys_env.iter().map(|(k, v)| (k.clone(), v.clone()));
        let overrides = self
            .env_stack
            .iter()
            .flat_map(|layer| layer.iter().map(|(k, v)| (k.clone(), v.clone())));
        sys.chain(overrides)
    }
}
