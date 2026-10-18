# std::process natives

The runtime provides the process kernel and the standard streams. A call
reaches it as `std::process::name(...)`.

- `spawn(argv: Vec<txt>) -> int`: start `argv` in its own process group,
  without a shell. The child inherits every stream. Returns the process id.
- `reap(pid: int) -> int`: wait for the process `pid` and return its raw
  `wait` status: the exit code in the high byte, the signal in the low seven
  bits. Returns the status.
- `signal_group(pid: int, signal: int)`: send the numbered signal to the
  process group `pid`. Produces no value.
- `write(file_descriptor: int, text: txt)`: write `text` to the standard
  stream `file_descriptor` (`1` is stdout, `2` is stderr). Produces no value.
- `is_terminal(file_descriptor: int) -> bool`: whether the standard stream
  `file_descriptor` is a terminal. Returns the boolean.
- `read_stdin() -> txt`: read all of standard input as text. Returns the text.
- `sleep_seconds(seconds: int)`: sleep for `seconds` whole seconds; a negative
  count stops the run. Produces no value.
- `var(name: txt) -> Option<txt>`: the value of the environment variable
  `name`, or the absence when it is unset. Returns the value.
- `current_dir() -> txt`: the process's current working directory. Returns the
  path.
