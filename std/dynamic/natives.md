# std::dynamic natives

The runtime keeps two containers itself: the vec and the hash map. Each is a
declared `rec` in this namespace, and its operations are the natives below. A
call reaches one as `std::dynamic::name(...)`, or as a method on its receiver:
`xs.at(index)`, `m.insert(key, value)`. A `new` is an associated function,
reached as `std::dynamic::Vec.new()` or `std::dynamic::HashMap.new()`.

## The vec

- `new() -> Vec<T>`: a new, empty vec. Returns the vec.
- `len(self: Vec<T>) -> int`: the number of elements. Returns the count.
- `at(self: Vec<T>, index: int) -> T`: the element at `index`. An index
  outside the vec stops the run. Returns the element.
- `replace(self: Vec<T>, index: int, item: T) -> Vec<T>`: a new vec with the
  element at `index` replaced. An index outside the vec stops the run; this
  never appends. Returns the new vec.
- `push(self: Vec<T>, item: T) -> Vec<T>`: a new vec with `item` appended.
  Returns the new vec.

## The hash map

A key is compared by structural equality, the same `==` every value uses, and
an entry is stored under that equality. A key may be any value.

- `new() -> HashMap<K, V>`: a new, empty hash map. Returns the map.
- `insert(self: HashMap<K, V>, key: K, value: V) -> HashMap<K, V>`: a new map
  with `value` stored under `key`. An existing key keeps its place and its
  value is replaced. Returns the new map.
- `find(self: HashMap<K, V>, key: K) -> Option<V>`: the value stored under
  `key`, or `std::Option.none` when the key is absent. Returns the option.
  `get`, `get_or`, and `contains` are kiru over this one lookup.
- `remove(self: HashMap<K, V>, key: K) -> HashMap<K, V>`: a new map without
  `key`. A key that is absent leaves the map unchanged. Returns the new map.
- `len(self: HashMap<K, V>) -> int`: the number of entries. Returns the count.
- `keys(self: HashMap<K, V>) -> Vec<K>`: the keys. Returns a new vec, in the
  table's own order.
- `values(self: HashMap<K, V>) -> Vec<V>`: the values. Returns a new vec, in
  the table's own order, aligned with `keys` for the same map value.
