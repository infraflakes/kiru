# std::dynamic natives

The runtime provides the vec's five operations. A call reaches them as
`std::dynamic::name(...)`, or as a method on a `Vec<T>` receiver:
`xs.at(index)`. `new` is a constructor, reached as `Vec.new()`.

- `new() -> Vec<T>`: a new, empty vec. Returns the vec.
- `len(self: Vec<T>) -> int`: the number of elements. Returns the count.
- `at(self: Vec<T>, index: int) -> T`: the element at `index`. An index
  outside the vec stops the run. Returns the element.
- `replace(self: Vec<T>, index: int, item: T) -> Vec<T>`: a new vec with the
  element at `index` replaced. An index outside the vec stops the run; this
  never appends. Returns the new vec.
- `push(self: Vec<T>, item: T) -> Vec<T>`: a new vec with `item` appended.
  Returns the new vec.
