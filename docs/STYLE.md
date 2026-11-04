# The Book's Style

*The Kiru Programming Language* does two jobs: it teaches the language, and it
is the reference a reader returns to. The book keeps those jobs apart with four
parts.

## The Four Parts

| Part | Pages | Job |
|---|---|---|
| Tutorial | Getting Started | Teach by doing. Every step is runnable, and the reader has something that works before the next idea. |
| Explanation | Concepts | Explain why the model is the way it is. Read after the tutorial, or when a rule surprises you. |
| How-to | Commands, Concurrency, Failure | Solve one task, with no theory beyond what the task needs. |
| Reference | Standard Library, Appendix | Look something up: every signature, native, and keyword. |

A page belongs to one part. A tutorial does not turn into a reference, and a
reference does not turn into a tutorial. When a page needs both, split it.

## Prose

- Address the reader as *you*. The book is a guide, not a specification.
- Present tense: "the compiler reports", not "the compiler will report".
- One idea per paragraph. A section answers one question.
- Start with the smallest complete example, then explain it. Do not state a
  rule before the reader has seen it happen.
- State a rule once, in the place it matters, and link to the concept page
  instead of restating it.
- Prefer italics for a term the first time it is defined: an *associated
  function*. Do not use single quotes for terms.
- Write a function name in prose without parentheses: `len`, not `len()`.
  Parentheses belong in code.
- Title case for headings. Code names inside a heading stay code: ``Reading with
  `get` ``.
- Hard wrap prose at 80 columns. Code blocks are exempt.
- No "simply", "just", or "obviously". A step is either explained or removed.

## Code

- Every `kiru` block is a complete program that compiles. The pipeline's
  `kiru book` check compiles every block, so a block that does not compile is a
  bug, not a fragment.
- Show the whole program for a first example. For a later variation, show the
  lines that changed and say what stayed.
- Comments carry the expected output: `std::print(x);   # 3`.
- Use `bash` fences for shell commands and their output, `text` for a signature
  or a shape, and `kiru` only for a program.
- Name the file above a block when the block is one file of a multi-file
  program.
- Keep code lines under 80 columns where the language allows it.

## Reference Pages

The standard library pages keep their `<details>` dropdowns: one function per
dropdown, closed by default, so the page stays scannable. A dropdown holds the
signature in a `text` block, the parameters, the return, and one `kiru`
example. Do not flatten the dropdowns into one long page; the closed list is
the overview.

Every public function, native, and type of that namespace appears, in the
order a reader reaches for it, not in declaration order.

## Links

- Intra-book links are absolute paths: `[Hash maps](/concepts/hash-maps/)`.
- A link names what the reader will find: `[Panic and
  Failure](/failure/panic/)`, not `[here](/failure/panic/)`.
- Do not link to a heading by anchor; headings change. Link to the page.

## Before a Page Lands

1. Every `kiru` block compiles: `./pipeline/main kiru book`.
2. The page belongs to one part of the four-part structure.
3. The first example is complete, and the reader can run it.
4. Every rule is stated once, and linked rather than repeated.
5. Headings are title case; code in a heading is code.
6. Prose is wrapped at 80 columns.
7. The site builds (`bun run build`), and the std dropdowns still open.
