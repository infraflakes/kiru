//! The runtime values: text, a record of text, or the no-value result of a
//! call that returns nothing.

use super::kinds::Kind;

/// A runtime value: text, a record of text, or the no-value result of a call
/// that returns nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    Text(String),
    Record(Record),
    List(Vec<String>),
    Nothing,
    /// An internal loop counter. Only loop lowering produces it and only the
    /// loop instructions read it, so the language never sees a number.
    Number(i64),
}

impl Kind {
    /// The kind a runtime value carries. This is the dynamic counterpart of
    /// the checker's static kind; only the kind round-trip test uses it now
    /// that the VM trusts the checker.
    #[cfg(test)]
    pub(crate) fn of(value: &Value) -> Kind {
        match value {
            Value::Text(_) => Kind::Text,
            Value::Record(_) => Kind::Record,
            Value::List(_) => Kind::List,
            Value::Nothing => Kind::Nothing,
            Value::Number(_) => unreachable!("a number is internal to loop lowering"),
        }
    }
}

/// A record of text fields in insertion order. A missing key reads as `""`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Record {
    entries: Vec<(String, String)>,
}

impl Record {
    /// Build a record from key/value pairs in order. A later duplicate key
    /// replaces an earlier one, exactly as `set` does.
    pub(crate) fn from_pairs<I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut record = Self::default();
        for (key, value) in pairs {
            record.set(key, value);
        }
        record
    }

    /// Replace any value already stored for `key`.
    pub(crate) fn set(&mut self, key: String, value: String) {
        match self.entries.iter_mut().find(|(name, _)| *name == key) {
            Some(entry) => entry.1 = value,
            None => self.entries.push((key, value)),
        }
    }

    /// Read a field, or `""` when it is absent.
    pub(crate) fn get(&self, key: &str) -> &str {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
            .unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A value that carries a concrete kind, for the round-trip test.
    fn value_of(kind: Kind) -> Value {
        match kind {
            Kind::Text => Value::Text(String::new()),
            Kind::Record => Value::Record(Record::default()),
            Kind::List => Value::List(Vec::new()),
            Kind::Nothing => Value::Nothing,
        }
    }

    #[test]
    fn every_concrete_kind_round_trips_through_a_value() {
        for kind in [Kind::Text, Kind::Record, Kind::List, Kind::Nothing] {
            assert_eq!(
                Kind::of(&value_of(kind)),
                kind,
                "{kind:?} did not round trip"
            );
        }
    }
}
