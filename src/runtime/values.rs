//! The runtime values: text, a map of values, a Vec of text, a boolean, an
//! integer, or the no-value result of a call that returns nothing.
//!
//! The kinds of value are not the runtime's to invent: the compiler's group
//! model declares the runtime shapes, and every `Value` carries one of them
//! and reads it (`Value::runtime_shape`) instead of any pass re-deriving a
//! kind. A new kind is a compile error at the declared shape first.
//!
//! The language has copy semantics: a value behaves as if it were copied on
//! every assignment, pass, and write. The representation makes the copy cheap
//! instead of eager — text, Vecs, maps, variant payloads, and closure
//! captures are refcounted and shared, and a write unshares first. A clone is
//! then constant time, and a value still behaves as if it were copied: no
//! mutation is ever visible through another reference.

use std::fmt;
use std::sync::{Arc, OnceLock};

use crate::compiler::model::RuntimeShape;

/// A runtime value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    Text(Text),
    Record(Map),
    Vec(Arc<Vec<Value>>),
    /// A constructed enum variant: its declared variant index and payload.
    Variant {
        tag: u16,
        payload: Arc<Vec<Value>>,
    },
    /// A top-level function used as a value: its lowered code id, plus the
    /// values a closure captured (empty for a named function).
    Function {
        code: u32,
        captures: Arc<Vec<Value>>,
    },
    Bool(bool),
    /// An integer. Loop lowering also uses it for a counter.
    Int(i64),
    /// The result of a call that returns nothing. The language never stores
    /// it; it only marks a call whose value a statement discards.
    Nothing,
}

impl Value {
    /// The declared shape this value carries, read once from the shape set
    /// the group model declares in `crate::compiler::model`. The runtime's
    /// `Value` names storage only: every operation that needs to tell a kind
    /// of value reads the shape below, so the runtime invents no kind beside
    /// the model's, and a kind the runtime gains without the model declaring
    /// it fails to compile here first.
    pub(crate) fn runtime_shape(&self) -> RuntimeShape {
        match self {
            Value::Text(_) => RuntimeShape::Text,
            Value::Record(_) => RuntimeShape::Record,
            Value::Vec(_) => RuntimeShape::List,
            Value::Variant { .. } => RuntimeShape::Variant,
            Value::Function { .. } => RuntimeShape::Function,
            Value::Bool(_) => RuntimeShape::Bool,
            Value::Int(_) => RuntimeShape::Int,
            Value::Nothing => RuntimeShape::Nothing,
        }
    }

    /// The text this value stores, or `None` when its shape is not the text
    /// shape.
    pub(crate) fn text(&self) -> Option<&Text> {
        if self.runtime_shape() != RuntimeShape::Text {
            return None;
        }
        let Value::Text(text) = self else {
            unreachable!("a value whose shape is the text shape stores a text");
        };
        Some(text)
    }

    /// The integer this value stores, or `None` when its shape is not the
    /// integer shape.
    pub(crate) fn integer(&self) -> Option<i64> {
        if self.runtime_shape() != RuntimeShape::Int {
            return None;
        }
        let Value::Int(number) = self else {
            unreachable!("a value whose shape is the integer shape stores an integer");
        };
        Some(*number)
    }

    /// The boolean this value stores, or `None` when its shape is not the
    /// boolean shape.
    pub(crate) fn boolean(&self) -> Option<bool> {
        if self.runtime_shape() != RuntimeShape::Bool {
            return None;
        }
        let Value::Bool(value) = self else {
            unreachable!("a value whose shape is the boolean shape stores a boolean");
        };
        Some(*value)
    }

    /// The record this value stores, or `None` when its shape is not the
    /// record shape.
    pub(crate) fn record(&self) -> Option<&Map> {
        if self.runtime_shape() != RuntimeShape::Record {
            return None;
        }
        let Value::Record(map) = self else {
            unreachable!("a value whose shape is the record shape stores a map");
        };
        Some(map)
    }

    /// The record this value stores, to change it, or `None` when its shape is
    /// not the record shape.
    pub(crate) fn record_mut(&mut self) -> Option<&mut Map> {
        if self.runtime_shape() != RuntimeShape::Record {
            return None;
        }
        let Value::Record(map) = self else {
            unreachable!("a value whose shape is the record shape stores a map");
        };
        Some(map)
    }

    /// The list this value stores, kept shared, or `None` when its shape is
    /// not the list shape.
    pub(crate) fn list(&self) -> Option<&Arc<Vec<Value>>> {
        if self.runtime_shape() != RuntimeShape::List {
            return None;
        }
        let Value::Vec(values) = self else {
            unreachable!("a value whose shape is the list shape stores a Vec");
        };
        Some(values)
    }

    /// The variant this value is: its declared tag and its payload, copied as
    /// a shared count, or `None` when its shape is not the variant shape.
    pub(crate) fn variant(&self) -> Option<(u16, Arc<Vec<Value>>)> {
        if self.runtime_shape() != RuntimeShape::Variant {
            return None;
        }
        let Value::Variant { tag, payload } = self else {
            unreachable!("a value whose shape is the variant shape stores a tag and payload");
        };
        Some((*tag, Arc::clone(payload)))
    }

    /// The function this value is: its lowered code id and the values it
    /// captured, copied as a shared count, or `None` when its shape is not
    /// the function shape.
    pub(crate) fn function(&self) -> Option<(u32, Arc<Vec<Value>>)> {
        if self.runtime_shape() != RuntimeShape::Function {
            return None;
        }
        let Value::Function { code, captures } = self else {
            unreachable!("a value whose shape is the function shape stores code and captures");
        };
        Some((*code, Arc::clone(captures)))
    }

    /// The text this value's shape renders: a text is itself, a boolean and an
    /// integer are their words, and no other shape renders, because the
    /// checker admits only these three into `@(…)`. The one reading the
    /// `ToText` instruction and every interpolation goes through.
    pub(crate) fn rendered(&self) -> Option<Text> {
        match self.runtime_shape() {
            RuntimeShape::Text => self.text().cloned(),
            RuntimeShape::Int => self.integer().map(|number| Text::from(number.to_string())),
            RuntimeShape::Bool => self.boolean().map(|value| Text::from(value.to_string())),
            RuntimeShape::Record
            | RuntimeShape::List
            | RuntimeShape::Variant
            | RuntimeShape::Function
            | RuntimeShape::Nothing
            | RuntimeShape::Parameter => None,
        }
    }

    /// The ordering two values compare by: two integers or two texts order as
    /// the one comparison the checker's admits, and every other pair orders
    /// nothing, because the checker never compares across shapes. A pair the
    /// facts leave unordered fails at the one reader of this.
    pub(crate) fn ordering(&self, right: &Self) -> Option<std::cmp::Ordering> {
        if let Some(left) = self.integer() {
            return right.integer().map(|right| left.cmp(&right));
        }
        match self.text() {
            Some(left) => right.text().map(|right| left.as_str().cmp(right.as_str())),
            None => None,
        }
    }

    /// Whether this value carries a value at all: the absence is the one
    /// shape that names no value, so the operations whose parameters take any
    /// value fit every shape but this one.
    pub(crate) fn carries(&self) -> bool {
        self.runtime_shape() != RuntimeShape::Nothing
    }
}

/// Runtime text, immutable and shared. A value is either a flat string or a
/// rope node naming a left and a right text. Concatenation builds a rope in
/// constant time, so building a string in a loop stays linear; the flat bytes
/// are produced once, on the first read, and cached. Text never mutates, so
/// sharing it never aliases observably.
#[derive(Clone)]
pub(crate) struct Text(Repr);

#[derive(Clone)]
enum Repr {
    Flat(Arc<String>),
    Rope(Arc<RopeNode>),
    /// A view into the bytes `[start, end)` of `source`. A slice shares its
    /// source instead of copying, so a slice is O(1) and many slices of one
    /// text stay one allocation; text is immutable, so sharing never aliases
    /// observably. This is what keeps a kiru `split` linear: each separator is
    /// searched in an O(1) view of the remainder, not a copy of it.
    Sub(Arc<String>, usize, usize),
    /// The empty text. It carries no allocation, so it can stand in for a rope
    /// child while a rope is dismantled iteratively.
    Empty,
}

struct RopeNode {
    left: Text,
    right: Text,
    len: usize,
    /// The flattened bytes, shared so a slice of a rope can be a view of them.
    flat: OnceLock<Arc<String>>,
}

impl Text {
    /// The bytes, flattening a rope on first read.
    pub(crate) fn as_str(&self) -> &str {
        match &self.0 {
            Repr::Flat(text) => text.as_str(),
            Repr::Rope(node) => node.flat.get_or_init(|| Arc::new(flatten(node))).as_str(),
            Repr::Sub(source, start, end) => &source[*start..*end],
            Repr::Empty => "",
        }
    }

    /// The number of bytes, without flattening a rope.
    pub(crate) fn byte_len(&self) -> usize {
        byte_len_of(&self.0)
    }

    /// The bytes in `[start, end)` as a text sharing this one's storage: O(1)
    /// for a flat or already-flattened text, and the flatten for a rope. A
    /// range outside the text, `start > end`, or a range that splits a
    /// character (text is a `String`, so its bytes are UTF-8) is an error the
    /// caller reports.
    pub(crate) fn slice(&self, start: usize, end: usize) -> Result<Text, String> {
        let len = self.byte_len();
        if start > end || end > len {
            return Err(format!(
                "slice [{start}, {end}) is outside text of {len} byte(s)"
            ));
        }
        if self.as_str().get(start..end).is_none() {
            return Err(format!("slice [{start}, {end}) splits a character"));
        }
        match &self.0 {
            Repr::Flat(source) => Ok(Text(Repr::Sub(Arc::clone(source), start, end))),
            Repr::Sub(source, base, _) => Ok(Text(Repr::Sub(
                Arc::clone(source),
                base + start,
                base + end,
            ))),
            Repr::Rope(node) => {
                let source = node.flat.get_or_init(|| Arc::new(flatten(node)));
                Ok(Text(Repr::Sub(Arc::clone(source), start, end)))
            }
            Repr::Empty => Ok(Text::empty()),
        }
    }

    /// The empty text.
    fn empty() -> Text {
        Text(Repr::Empty)
    }

    /// `left` followed by `right`, in constant time. An empty side is dropped,
    /// so concatenation never builds a node around nothing.
    pub(crate) fn concat(left: Text, right: Text) -> Text {
        match (left.0, right.0) {
            (Repr::Empty, right) => Text(right),
            (left, Repr::Empty) => Text(left),
            (left, right) => {
                let len = byte_len_of(&left) + byte_len_of(&right);
                Text(Repr::Rope(Arc::new(RopeNode {
                    left: Text(left),
                    right: Text(right),
                    len,
                    flat: OnceLock::new(),
                })))
            }
        }
    }
}

/// The byte length of a representation, for `Text::concat`.
fn byte_len_of(repr: &Repr) -> usize {
    match repr {
        Repr::Flat(text) => text.len(),
        Repr::Rope(node) => node.len,
        Repr::Sub(_, start, end) => end - start,
        Repr::Empty => 0,
    }
}

/// Dismantle rope children iteratively when a node is freed. A long chain of
/// concatenations is a deep left-leaning tree, and the default recursive drop
/// would overflow the stack; this frees it in one loop.
impl Drop for RopeNode {
    fn drop(&mut self) {
        // A node whose children were already taken has nothing to dismantle.
        if matches!(self.left.0, Repr::Empty) && matches!(self.right.0, Repr::Empty) {
            return;
        }
        let mut stack = vec![
            std::mem::replace(&mut self.left, Text::empty()),
            std::mem::replace(&mut self.right, Text::empty()),
        ];
        while let Some(text) = stack.pop() {
            match text.0 {
                Repr::Rope(node) => {
                    if let Ok(mut node) = Arc::try_unwrap(node) {
                        stack.push(std::mem::replace(&mut node.left, Text::empty()));
                        stack.push(std::mem::replace(&mut node.right, Text::empty()));
                    }
                }
                Repr::Flat(_) | Repr::Sub(..) | Repr::Empty => {}
            }
        }
    }
}

impl From<String> for Text {
    fn from(text: String) -> Self {
        Text(Repr::Flat(Arc::new(text)))
    }
}

impl From<&str> for Text {
    fn from(text: &str) -> Self {
        Text(Repr::Flat(Arc::new(text.to_owned())))
    }
}

impl std::ops::Deref for Text {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq for Text {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for Text {}

impl fmt::Debug for Text {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), formatter)
    }
}

/// One piece of a rope, for the iterative flatten. A leaf is its bytes; a node
/// is walked in order.
enum Piece<'a> {
    Flat(&'a str),
    Node(&'a RopeNode),
}

/// The flat bytes of a rope, produced in one pass.
fn flatten(node: &RopeNode) -> String {
    let mut out = String::with_capacity(node.len);
    let mut stack = vec![Piece::Node(node)];
    while let Some(piece) = stack.pop() {
        match piece {
            Piece::Flat(text) => out.push_str(text),
            Piece::Node(node) => {
                stack.push(piece_of(&node.right));
                stack.push(piece_of(&node.left));
            }
        }
    }
    out
}

fn piece_of(text: &Text) -> Piece<'_> {
    match &text.0 {
        Repr::Flat(text) => Piece::Flat(text.as_str()),
        Repr::Rope(node) => Piece::Node(node),
        Repr::Sub(source, start, end) => Piece::Flat(&source[*start..*end]),
        Repr::Empty => Piece::Flat(""),
    }
}

/// A map of named fields in insertion order. A field holds any value, and the
/// checker types it from the declared `rec` whose literal produced the map. A
/// missing key reads as absent.
#[derive(Clone, Debug, Default, Eq)]
pub(crate) struct Map {
    entries: Arc<Vec<(String, Value)>>,
}

/// A map holds the same value as another when it carries the same
/// key/value pairs whatever the field order. Duplicate keys never occur: `set`
/// replaces an earlier entry.
impl PartialEq for Map {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self
                .entries
                .iter()
                .all(|(key, value)| other.get(key) == Some(value))
    }
}

impl Map {
    /// Replace any value already stored for `key`.
    pub(crate) fn set(&mut self, key: String, value: Value) {
        let entries = Arc::make_mut(&mut self.entries);
        match entries.iter_mut().find(|(name, _)| *name == key) {
            Some(entry) => entry.1 = value,
            None => entries.push((key, value)),
        }
    }

    /// Read a field, or `None` when it is absent.
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_value_carries_the_shape_the_model_declares() {
        // The one reading between storage and the model: every shape the
        // runtime can hold is declared, and the absence is the one shape
        // that names no value. The type parameter is the only shape of the
        // model's set no runtime value can carry.
        let values = [
            (Value::Text(Text::from("a")), RuntimeShape::Text),
            (Value::Record(Map::default()), RuntimeShape::Record),
            (Value::Vec(Arc::new(Vec::new())), RuntimeShape::List),
            (
                Value::Variant {
                    tag: 0,
                    payload: Arc::new(Vec::new()),
                },
                RuntimeShape::Variant,
            ),
            (
                Value::Function {
                    code: 0,
                    captures: Arc::new(Vec::new()),
                },
                RuntimeShape::Function,
            ),
            (Value::Bool(true), RuntimeShape::Bool),
            (Value::Int(1), RuntimeShape::Int),
            (Value::Nothing, RuntimeShape::Nothing),
        ];
        assert_eq!(values.len(), 8, "the runtime stores eight shapes");
        for (value, shape) in values {
            assert_eq!(value.runtime_shape(), shape, "each shape read once");
            assert_eq!(
                value.carries(),
                shape != RuntimeShape::Nothing,
                "only the absence carries no value"
            );
        }
    }
}
