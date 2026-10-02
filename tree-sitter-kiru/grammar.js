/**
 * Tree-sitter grammar for the kiru language.
 *
 * The grammar follows the accepted syntax in the language spec and the
 * appendix grammar. A file opens with an optional `module` path, then imports,
 * then declarations. Every declaration and every statement ends with `;`,
 * including a braced block used as a statement. There are no numeric literals:
 * text is the only scalar, and a string may span lines.
 *
 * The rule shapes mirror the reference grammar:
 *
 *   file        := module? import* declaration*
 *   declaration := function | text | record
 *   statement   := text | record | assignment | expression ";"
 *                | "return" expression? ";"
 *                | "panic" ";"
 *                | "switch" "(" expression ")" switch ";"
 *                | "defer" block ";"
 *   expression  := term ("+" term)*
 *   term        := primary ("." ident)*
 *   primary     := string | fields | path arguments? | path
 *
 * Names are `::` qualified and may open with `::` to name the root namespace.
 *
 * @file Tree-sitter grammar for kiru.
 * @license MIT
 */

module.exports = grammar({
  name: 'kiru',

  // Reserving the identifier rule lets the lexer treat the language keywords
  // as whole words, so `switch` never lexes as an identifier prefix.
  word: $ => $.identifier,

  extras: $ => [
    $.comment,
    /[\s\p{Zs}]/,
  ],

  rules: {
    // A whole source file: an optional module, imports, then declarations.
    source_file: $ => seq(
      optional($.module_declaration),
      repeat($.import_declaration),
      repeat($._declaration),
    ),

    // `module a::b::c;` names the file namespace. The reference grammar
    // routes it through `path`, so a leading `::` is rejected later by the
    // compiler rather than by the parser.
    module_declaration: $ => seq(
      'module',
      field('path', $.path),
      ';',
    ),

    // `import "relative/or/absolute/path";`
    import_declaration: $ => seq(
      'import',
      field('path', $.string),
      ';',
    ),

    // A top level declaration. `fn` introduces a function; `txt` and `rec`
    // bind a value or a record.
    _declaration: $ => choice(
      $.function_declaration,
      $.text_binding,
      $.record_binding,
    ),

    // `fn name(txt a, rec b) { ... };`
    function_declaration: $ => seq(
      'fn',
      field('name', $.identifier),
      field('parameters', $.parameter_list),
      field('body', $.block),
      ';',
    ),

    // A comma separated parameter list; a trailing comma is allowed.
    parameter_list: $ => seq(
      '(',
      optional(seq(
        $.parameter,
        repeat(seq(',', $.parameter)),
        optional(','),
      )),
      ')',
    ),

    // A parameter writes its kind before its name; this is the only place a
    // kind is written.
    parameter: $ => seq(
      field('kind', choice('txt', 'rec')),
      field('name', $.identifier),
    ),

    // `txt name = expression;` at the top level or as a local binding.
    text_binding: $ => seq(
      'txt',
      field('name', $.identifier),
      '=',
      field('value', $.expression),
      ';',
    ),

    // `rec name = { key = expression, ... };` at the top level or as a local.
    record_binding: $ => seq(
      'rec',
      field('name', $.identifier),
      '=',
      field('value', $.record_literal),
      ';',
    ),

    // `{ statement* }`, used as a function body and as the body of `switch`
    // arms and `defer`. A block is always followed by the statement
    // semicolon at its use site.
    block: $ => seq(
      '{',
      repeat($._statement),
      '}',
    ),

    // A statement inside a block.
    _statement: $ => choice(
      $.text_binding,
      $.record_binding,
      $.assignment_statement,
      $.return_statement,
      $.panic_statement,
      $.switch_statement,
      $.defer_statement,
      $.expression_statement,
    ),

    // `name = expression;` redefines a local binding.
    assignment_statement: $ => seq(
      field('name', $.identifier),
      '=',
      field('value', $.expression),
      ';',
    ),

    // `return;` ends a void function, `return expression;` returns a value.
    return_statement: $ => seq(
      'return',
      optional(field('value', $.expression)),
      ';',
    ),

    // `panic;` ends the run.
    panic_statement: $ => seq(
      'panic',
      ';',
    ),

    // `defer { ... };` registers a body to run when the enclosing body ends.
    defer_statement: $ => seq(
      'defer',
      field('body', $.block),
      ';',
    ),

    // `switch(subject) { case(...) { ... }; default { ... }; };`
    switch_statement: $ => seq(
      'switch',
      '(',
      field('subject', $.expression),
      ')',
      '{',
      repeat(choice($.case_clause, $.default_clause)),
      '}',
      ';',
    ),

    // `case(pattern) { ... };`
    case_clause: $ => seq(
      'case',
      '(',
      field('pattern', $.expression),
      ')',
      field('body', $.block),
      ';',
    ),

    // `default { ... };`
    default_clause: $ => seq(
      'default',
      field('body', $.block),
      ';',
    ),

    // A bare expression used as a statement, always closed with `;`.
    expression_statement: $ => seq(
      $.expression,
      ';',
    ),

    // Sums of terms; `+` is left associative and concatenates text.
    expression: $ => choice(
      $.binary_expression,
      $._term,
    ),

    binary_expression: $ => prec.left(1, seq(
      field('left', $.expression),
      field('operator', '+'),
      field('right', $.expression),
    )),

    // A primary with any number of field accesses.
    _term: $ => choice(
      $.call_expression,
      $.field_access,
      $.string,
      $.record_literal,
      $.path,
    ),

    // `path(arguments)` calls a function.
    call_expression: $ => prec(2, seq(
      field('callee', $.path),
      field('arguments', $.argument_list),
    )),

    // `target.field` reads a record field.
    field_access: $ => prec.left(3, seq(
      field('target', $._term),
      '.',
      field('field', $.identifier),
    )),

    // A comma separated argument list; a trailing comma is allowed.
    argument_list: $ => seq(
      '(',
      optional(seq(
        $.expression,
        repeat(seq(',', $.expression)),
        optional(','),
      )),
      ')',
    ),

    // `{ key = expression, ... }`; fields are text, so a record does not
    // nest and a duplicate key is resolved by the compiler.
    record_literal: $ => seq(
      '{',
      optional(seq(
        $.record_field,
        repeat(seq(',', $.record_field)),
        optional(','),
      )),
      '}',
    ),

    // One `key = expression` field of a record literal.
    record_field: $ => seq(
      field('name', $.identifier),
      '=',
      field('value', $.expression),
    ),

    // A `::` qualified name with an optional leading `::`. Every segment but
    // the last is a namespace; the last segment names the value or function.
    path: $ => seq(
      optional($.root_namespace),
      repeat(seq($.namespace, '::')),
      field('name', $.identifier),
    ),

    // The leading `::` that roots a path at the root namespace.
    root_namespace: $ => '::',

    // A non final segment of a qualified path.
    namespace: $ => $.identifier,

    identifier: _ => /[a-zA-Z_][a-zA-Z0-9_]*/,

    // A double quoted string. A newline is an ordinary character, so strings
    // span lines; only the five spec escapes are recognized.
    string: _ => token(seq(
      '"',
      repeat(choice(
        /[^"\\]/,
        /\\(?:n|t|r|\\|")/,
      )),
      '"',
    )),

    // A `#` comment running to the end of the line.
    comment: _ => token(seq('#', /[^\n]*/)),
  },
});
