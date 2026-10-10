//! The runtime values: text, a record of text, a list of text, the no-value
//! result of a call that returns nothing, or an internal loop counter.

/// A runtime value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    Text(String),
    Record(Record),
    List(Vec<String>),
    /// The result of a call that returns nothing. The language never stores
    /// it; it only marks a call whose value a statement discards.
    Nothing,
    /// An internal loop counter. Only loop lowering produces it and only the
    /// loop instructions read it, so the language never sees a number.
    Number(i64),
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
