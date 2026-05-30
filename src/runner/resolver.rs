use super::context::ExecContext;
use crate::dsl::ast::Expr;
use crate::runner::error::RuntimeError;

impl<'a> ExecContext<'a> {
    /// Resolve an expression, checking scope layers before base vars.
    pub(super) fn resolve_expr(&self, expr: &Expr) -> Result<String, RuntimeError> {
        match expr {
            Expr::VarRef { name } => self
                .lookup_var(name)
                .cloned()
                .ok_or_else(|| RuntimeError::Lookup(format!("undefined variable: ${}", name))),
            Expr::BacktickLit { parts } => {
                let mut result = String::new();
                for part in parts {
                    if part.is_var {
                        let val = self.lookup_var(&part.value).ok_or_else(|| {
                            RuntimeError::Lookup(format!("undefined variable: ${}", part.value))
                        })?;
                        result.push_str(val);
                    } else {
                        result.push_str(&part.value);
                    }
                }
                Ok(result)
            }
        }
    }

    /// Look up a variable, checking scope layers (top-to-bottom) then base vars.
    fn lookup_var(&self, name: &str) -> Option<&String> {
        for layer in self.var_stack.iter().rev() {
            if let Some(val) = layer.get(name) {
                return Some(val);
            }
        }
        self.vars.get(name)
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
