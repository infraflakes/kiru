//! The list natives: length, indexing, appending, and ordering.
//!
//! A list holds text elements. `append`, `prepend`, and `sort` answer a new
//! list; `get` fails the run when the index is malformed or outside the list.

use crate::model::Value;

use super::parse_whole_number;

/// The number of elements in `list`.
pub(crate) fn len(list: &[String]) -> Value {
    Value::Text(list.len().to_string())
}

/// The element at `index`. A malformed index, or one outside the list, fails
/// the run.
pub(crate) fn get(list: &[String], index: &str) -> Result<Value, String> {
    let index = parse_whole_number(index)?;
    match list.get(index) {
        Some(element) => Ok(Value::Text(element.clone())),
        None => Err(format!(
            "index {index} is outside a list of {} element(s)",
            list.len()
        )),
    }
}

/// A new list with `text` appended.
pub(crate) fn append(list: &[String], text: &str) -> Value {
    let mut list = list.to_vec();
    list.push(text.to_owned());
    Value::List(list)
}

/// A new list with `text` inserted at the front.
pub(crate) fn prepend(list: &[String], text: &str) -> Value {
    let mut result = Vec::with_capacity(list.len() + 1);
    result.push(text.to_owned());
    result.extend_from_slice(list);
    Value::List(result)
}

/// A new list with the elements in lexicographic order.
pub(crate) fn sort(list: &[String]) -> Value {
    let mut list = list.to_vec();
    list.sort();
    Value::List(list)
}
