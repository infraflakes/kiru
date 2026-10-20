# std::syscalls natives

The runtime provides the kernel: every native that touches the OS. A call
reaches it as `std::syscalls::name(...)`, or as a method on the first
parameter's type: `fd.write(text)`, `path.unlink()`. Every one of these names
the operation it wraps, so the kernel is the syscall floor: everything above it
is kiru.

- `write(file_descriptor: int, text: txt)`: write `text` to `file_descriptor`
  (`1` is stdout, `2` is stderr; any descriptor is accepted and the kernel
  answers `EBADF` for one it does not know). Produces no value.
- `unlink(path: txt)`: remove a file. Produces no value.
- `access(path: txt) -> bool`: whether a path exists. Returns the boolean.
- `mkstemp(template: txt) -> txt`: create a unique empty file from `template`,
  whose name must end in six `X`s, and return its path. The caller owns the
  file and must remove it. Returns the path.
- `readdir(path: txt) -> Vec<txt>`: the entry names of a directory, excluding
  `.` and `..`, in sorted order. A directory that cannot be read stops the
  run. Returns the names.
- `getenv(name: txt) -> Option<txt>`: the value of the environment variable
  `name`, or `std::Option.none` when it is unset. Returns the value.
- `getcwd() -> txt`: the process's current working directory. Returns the
  path.
- `isatty(file_descriptor: int) -> bool`: whether `file_descriptor` is a
  terminal. Any descriptor is accepted; the kernel answers `EBADF` for one it
  does not know. Returns the boolean.
- `read_stdin() -> txt`: read all of standard input as text. Returns the text.
- `posix_spawn(argv: Vec<txt>) -> int`: start `argv` in its own process group,
  without a shell. The child inherits every stream. Returns the process id.
- `waitpid(pid: int) -> int`: wait for the process `pid` and return its raw
  `wait` status: the exit code in the high byte, the signal in the low seven
  bits. Returns the status.
- `killpg(pid: int, signal: int)`: send the numbered signal to the process
  group `pid`. Produces no value.
- `nanosleep(seconds: int)`: sleep for `seconds` whole seconds; a negative
  count stops the run. Produces no value.
