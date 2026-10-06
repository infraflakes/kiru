//! How a body or a statement ends.

/// The ways one body or statement can end. A path either returns a value,
/// falls through, or breaks out of its enclosing loop; a path that does none of
/// these stops the run, so "stops" is the absence of all three. The outcomes
/// are not exclusive: a switch may have one arm return and another fall
/// through.
#[derive(Clone, Copy)]
pub(in crate::compiler::validate_program) struct Flow {
    pub(in crate::compiler::validate_program) returns: bool,
    pub(in crate::compiler::validate_program) falls: bool,
    pub(in crate::compiler::validate_program) breaks: bool,
}

impl Flow {
    /// A path that reaches the end of the body.
    pub(super) const FALLS: Flow = Flow {
        returns: false,
        falls: true,
        breaks: false,
    };
    /// A path that returns a value.
    pub(super) const RETURNS: Flow = Flow {
        returns: true,
        falls: false,
        breaks: false,
    };
    /// A path that ends the body without returning, falling, or breaking: it
    /// stops the run. This is also the union identity, since a set of no paths
    /// has no outcome.
    pub(super) const STOPS: Flow = Flow {
        returns: false,
        falls: false,
        breaks: false,
    };
    /// A path that leaves its enclosing loop.
    pub(super) const BREAKS: Flow = Flow {
        returns: false,
        falls: false,
        breaks: true,
    };

    /// The union of two flows: an outcome present in either is present. This
    /// is how the arms of a `switch` combine.
    pub(super) fn union(self, other: Flow) -> Flow {
        Flow {
            returns: self.returns || other.returns,
            falls: self.falls || other.falls,
            breaks: self.breaks || other.breaks,
        }
    }
}
