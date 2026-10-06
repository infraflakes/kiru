//! The list natives: length, indexing, appending, and ordering.
//!
//! A list holds text elements. `append`, `reverse`, and `sort` answer a new
//! list; `get` answers `""` when the index is outside the list.

use crate::model::Value;

/// The number of elements in `list`.
pub(crate) fn len(list: &[String]) -> Value {
    Value::Text(list.len().to_string())
}

/// The element at `index`, or `""` when the index is malformed or outside the
/// list.
pub(crate) fn get(list: &[String], index: &str) -> Value {
    let element = index
        .parse::<usize>()
        .ok()
        .and_then(|index| list.get(index))
        .cloned()
        .unwrap_or_default();
    Value::Text(element)
}

/// A new list with `text` appended.
pub(crate) fn append(list: &[String], text: &str) -> Value {
    let mut list = list.to_vec();
    list.push(text.to_owned());
    Value::List(list)
}

/// A new list with the elements reversed.
pub(crate) fn reverse(list: &[String]) -> Value {
    let mut list = list.to_vec();
    list.reverse();
    Value::List(list)
}

/// A new list with the elements in lexicographic order.
pub(crate) fn sort(list: &[String]) -> Value {
    let mut list = list.to_vec();
    list.sort();
    Value::List(list)
}
