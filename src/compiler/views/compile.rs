//! The compile view: the declarations the entry reaches, over stable ids.

mod mark_reachable;
mod view;

#[cfg(test)]
mod tests;

pub(crate) use mark_reachable::mark_reachable;
pub(crate) use view::verify_reachable;
