//! The one visibility relation: whether a declaration, member, or type in one
//! namespace may be reached from another.
//!
//! A name in its own namespace is always reachable. Across namespaces it is
//! reachable only when it is public. A declaration kind that takes no
//! visibility never crosses a namespace because its public flag is never set;
//! the kind fact decides whether the surface may write `pub`, and this relation
//! decides what `pub` reaches.

use super::super::ids::NamespaceId;

/// Whether something declared in `declaring` is reachable from `caller`.
pub(crate) fn reachable(public: bool, declaring: NamespaceId, caller: NamespaceId) -> bool {
    caller == declaring || public
}
