# std::text natives

The runtime provides the pure byte kernel: the four operations that read text
without touching the OS. A call reaches them as `std::text::name(...)`, or as a
method on a text: `s.strlen()`, `s.substr(start, end)`.

- `strlen(self: txt) -> int`: the number of bytes. Returns the count.
- `memchr(self: txt, needle: txt) -> Option<int>`: the byte index of the first
  occurrence of `needle`, or `std::Option.none` when it is absent. Returns the index.
- `substr(self: txt, start: int, end: int) -> txt`: the bytes in `[start, end)`.
  A negative index, an index outside the text, `start > end`, or a range that
  splits a character stops the run. Returns the text.
- `byte_at(self: txt, index: int) -> int`: the byte at `index`. An index
  outside the text, or a negative one, stops the run. Returns the byte.
