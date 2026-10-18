# std::text natives

The runtime provides the byte kernel. A call reaches it as
`std::text::name(...)`, or as a method on a `txt` receiver: `s.byte(index)`.

- `len(self: txt) -> int`: the number of bytes. Returns the count.
- `find_byte(self: txt, needle: txt) -> Option<int>`: the byte index of the
  first occurrence of `needle`, or the absence when it is absent. Returns the
  index.
- `byte(self: txt, index: int) -> int`: the byte at `index`. An index outside
  the text, or a negative one, stops the run. Returns the byte.
- `slice(self: txt, start: int, end: int) -> txt`: the bytes in
  `[start, end)`. A negative index, an index outside the text, `start > end`,
  or a range that splits a character stops the run. Returns the text.
