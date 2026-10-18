# std::fs natives

The runtime provides the file kernel. A call reaches it as
`std::fs::name(...)`, or as a method on a `txt` receiver: `path.read()`.

- `read(path: txt) -> txt`: read a whole file as text. A path that cannot be
  read, or whose bytes are not UTF-8, stops the run. Returns the text.
- `write(path: txt, text: txt)`: replace a file with `text`, creating it when
  it is absent. Produces no value.
- `remove(path: txt)`: remove a file. Produces no value.
- `exists(path: txt) -> bool`: whether a path exists. Returns the boolean.
- `mkstemp(template: txt) -> txt`: create a unique empty file from `template`,
  whose name must end in six `X`s, and return its path. The caller owns the
  file and must remove it. Returns the path.
- `list(path: txt) -> Vec<txt>`: the entry names of a directory, excluding `.`
  and `..`, in sorted order. A directory that cannot be read stops the run.
  Returns the names.
