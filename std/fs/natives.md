# std::fs natives

The runtime provides the file operations: reading and writing a path. A call
reaches them as `std::fs::name(...)`, or as a method on a path: `path.read()`,
`path.write(text)`.

- `read(path: txt) -> txt`: read a whole file as text. A path that cannot be
  read, or whose bytes are not UTF-8, stops the run. Returns the text.
- `write(path: txt, text: txt)`: replace a file with `text`, creating it when
  it is absent. Produces no value.
