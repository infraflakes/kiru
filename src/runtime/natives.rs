//! The native kernel: the small set of operations the OS must provide.
//!
//! Output, files, and text are the areas gathered here; the process and time
//! natives live in the process kernel. A handler answers with a runtime value,
//! or with a reason text that the virtual machine reports under the native's
//! own name.

pub(crate) mod environment;
pub(crate) mod file_system;
pub(crate) mod terminal_output;
pub(crate) mod text_operations;

use std::sync::Arc;

use crate::runtime::Value;

/// The tag of `std::Option`'s `some` variant, in the order the standard
/// library's declaration names its variants. A native whose result is an
/// option builds that variant directly, exactly as a `std::Option.some(...)`
/// construction lowers, so the runtime needs the order without a program.
pub(crate) const OPTION_SOME: u16 = 0;

/// The tag of `std::Option`'s `none` variant.
pub(crate) const OPTION_NONE: u16 = 1;

/// The `some` variant of `std::Option` around `value`.
pub(crate) fn option_some(value: Value) -> Value {
    Value::Variant {
        tag: OPTION_SOME,
        payload: Arc::new(vec![value]),
    }
}

/// The `none` variant of `std::Option`.
pub(crate) fn option_none() -> Value {
    Value::Variant {
        tag: OPTION_NONE,
        payload: Arc::new(Vec::new()),
    }
}
