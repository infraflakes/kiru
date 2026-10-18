//! Link-time receiver typing.
//!
//! The type of a method receiver, from declared types alone. kiru annotates
//! every binding, parameter, and return, so this is exact for a name and a
//! declared field; elsewhere it gives the head, which is all method dispatch
//! keys on. The checker then re-verifies the desugared call, so an
//! approximation can never be unsound.

use crate::compiler::model::{
    Binding, DeclarationId, Expression, MatchBody, Type, rec_field, substitute,
};
use crate::syntax::{BinaryOp, UnaryOp};

use super::builder::Builder;

impl Builder {
    /// The type of a method receiver, from declared types alone. kiru annotates
    /// every binding, parameter, and return, so this is exact for a name and a
    /// declared field; elsewhere it gives the head (a `Vec.new()` call is
    /// `Vec`, and so on), which is all method dispatch keys on. The checker
    /// then re-verifies the desugared call, so an approximation can never be
    /// unsound.
    pub(in crate::compiler::resolve_names) fn receiver_type(
        &self,
        expression: &Expression,
    ) -> Option<Type> {
        match expression {
            Expression::Text { .. } | Expression::Interpolated { .. } => Some(Type::Text),
            Expression::Int { .. } => Some(Type::Int),
            Expression::Bool { .. } => Some(Type::Bool),
            Expression::Map { .. } => None,
            Expression::Variant { ty, .. } => Some(Type::Named(*ty, Vec::new())),
            Expression::Reference { declaration, .. } => self.declared_type(*declaration),
            Expression::Call { callee, .. } => self.receiver_type(callee)?.return_type(&self.types),
            Expression::Field { target, name, .. } => {
                let target = self.receiver_type(target)?;
                self.field_type(&target, name)
            }
            Expression::Binary { op, .. } => Some(match op {
                BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Remainder => Type::Int,
                _ => Type::Bool,
            }),
            Expression::Unary { op, .. } => Some(match op {
                UnaryOp::Negate => Type::Int,
                UnaryOp::Not => Type::Bool,
            }),
            Expression::Match { cases, default, .. } => {
                for arm in cases {
                    if let MatchBody::Expression(expression) = &arm.body {
                        return self.receiver_type(expression);
                    }
                }
                match default {
                    Some(MatchBody::Expression(expression)) => self.receiver_type(expression),
                    _ => None,
                }
            }
            // A function literal has no methods; dispatch does not key on it.
            Expression::Function(_) => None,
        }
    }

    /// The declared type of a name, whatever its body produces.
    fn declared_type(&self, declaration: DeclarationId) -> Option<Type> {
        Some(self.declarations[declaration.0].as_ref()?.ty.clone())
    }

    /// The type of a field, substituting a declared type's arguments.
    fn field_type(&self, target: &Type, name: &str) -> Option<Type> {
        let Type::Named(id, arguments) = target else {
            return None;
        };
        let field = rec_field(&self.types, *id, name)?;
        Some(substitute(
            &field.ty,
            &self.types,
            &self.types[id.0].parameters,
            Binding::Complete(arguments),
        ))
    }
}
