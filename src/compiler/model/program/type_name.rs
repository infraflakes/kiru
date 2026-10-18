//! Rendering a type for a diagnostic.

use crate::compiler::model::{Type, type_name};

use super::graph::Program;

impl Program {
    /// The name of a type for a diagnostic. The rendering is a fact of the
    /// type's group, declared once beside every other fact of the kind.
    pub(crate) fn type_name(&self, ty: &Type) -> String {
        type_name(ty, &self.types)
    }
}
