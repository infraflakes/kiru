//! The kind system: every value a program can produce, and where each kind
//! may stand.
//!
//! Text and record are the only data kinds. `Nothing` is the no-value result
//! of a void call. Flow terminators such as `return` and `panic` are
//! statements, not values, so no kind marks them.
//!
//! Compatibility is a subset relation over usage sets. Every concrete kind is
//! one bit, group constants name the combinations a position or a call asks
//! for, and a value fits a requirement when its usage is contained in it.

/// A static kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) enum Kind {
    Text,
    Record,
    Nothing,
}

impl Kind {
    /// The name the diagnostics use.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Record => "record",
            Kind::Nothing => "nothing",
        }
    }

    /// The one-bit usage a concrete kind carries.
    pub(crate) fn usage(self) -> Usage {
        match self {
            Kind::Text => Usage::TEXT,
            Kind::Record => Usage::RECORD,
            Kind::Nothing => Usage::NOTHING,
        }
    }
}

/// A set of kinds, one bit per concrete kind. A value fits a requirement when
/// every bit it carries is also required.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Usage(u8);

impl Usage {
    /// A lone text value.
    pub(crate) const TEXT: Usage = Usage(1 << 0);
    /// A lone record value.
    pub(crate) const RECORD: Usage = Usage(1 << 1);
    /// The no-value result of a void call.
    pub(crate) const NOTHING: Usage = Usage(1 << 2);

    /// The data a program stores or reads: text and record.
    pub(crate) const DATA: Usage = Usage::TEXT.union(Usage::RECORD);
    /// Every kind a bare statement may discard.
    pub(crate) const DISCARDABLE: Usage = Usage::TEXT.union(Usage::RECORD).union(Usage::NOTHING);

    /// The union of two usages.
    pub(crate) const fn union(self, other: Usage) -> Usage {
        Usage(self.0 | other.0)
    }

    /// The name of a requirement, for diagnostics. A multi-bit group names
    /// its members; a single-bit usage names its one kind.
    pub(crate) fn name(self) -> &'static str {
        if self == Usage::TEXT {
            "text"
        } else if self == Usage::RECORD {
            "record"
        } else if self == Usage::NOTHING {
            "nothing"
        } else if self == Usage::DATA {
            "text or record"
        } else {
            "value"
        }
    }
}

/// Whether every kind an actual usage carries is also required. An actual
/// must be a subset of the requirement; an empty actual cannot occur because
/// every kind and every position sets at least one bit.
pub(crate) fn fits(actual: Usage, required: Usage) -> bool {
    actual.0 & required.0 == actual.0
}

/// A place a value can stand. Each position names the usage it requires, and
/// a value may stand there only when its own usage is contained in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Position {
    /// The initializer of a `txt` binding, module-level or local.
    TextBinding,
    /// The initializer of a `rec` binding, module-level or local.
    RecordBinding,
    /// A record field value.
    RecordField,
    /// A returned expression.
    Return,
    /// A bare statement in a body.
    Statement,
    /// A `case` pattern or a `switch` subject.
    CasePattern,
}

impl Position {
    /// The usage a value must fit to stand here.
    pub(crate) fn required_usage(self) -> Usage {
        match self {
            Position::TextBinding => Usage::TEXT,
            Position::RecordBinding => Usage::RECORD,
            Position::RecordField => Usage::TEXT,
            Position::Return => Usage::DATA,
            Position::Statement => Usage::DISCARDABLE,
            Position::CasePattern => Usage::TEXT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONCRETE_KINDS: &[Kind] = &[Kind::Text, Kind::Record, Kind::Nothing];

    const EVERY_POSITION: &[Position] = &[
        Position::TextBinding,
        Position::RecordBinding,
        Position::RecordField,
        Position::Return,
        Position::Statement,
        Position::CasePattern,
    ];

    #[test]
    fn every_concrete_kind_has_a_non_zero_usage() {
        for kind in CONCRETE_KINDS {
            assert_ne!(kind.usage().0, 0, "{kind:?} has no usage bit");
        }
    }

    #[test]
    fn every_position_requires_a_non_zero_usage() {
        for position in EVERY_POSITION {
            assert_ne!(
                position.required_usage().0,
                0,
                "{position:?} requires no usage"
            );
        }
    }

    #[test]
    fn nothing_is_accepted_only_as_a_statement() {
        for position in EVERY_POSITION {
            let statement = *position == Position::Statement;
            assert_eq!(
                fits(Usage::NOTHING, position.required_usage()),
                statement,
                "nothing accepted at {position:?}"
            );
        }
    }
}
