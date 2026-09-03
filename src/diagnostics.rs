use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

/// Byte span in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub offset: usize,
    pub len: usize,
}

impl Span {
    pub const fn new(offset: usize, len: usize) -> Self {
        Self { offset, len }
    }
}

/// A diagnostic with file, primary span, message, and source snapshot.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: String,
    pub primary: Span,
    pub message: String,
    pub source: String,
}

impl Diagnostic {
    pub fn new(
        file: impl Into<String>,
        span: Span,
        msg: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            file: file.into(),
            primary: span,
            message: msg.into(),
            source: source.into(),
        }
    }
}

fn is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

/// Render a diagnostic to a string using annotate-snippets.
pub fn render_diagnostic(diag: &Diagnostic) -> String {
    let src = diag.source.as_str();
    let src_len = src.len();

    // Find the primary error line (0-indexed) from the span offset.
    let error_line = src[..diag.primary.offset.min(src_len)]
        .bytes()
        .filter(|b| *b == b'\n')
        .count();

    // Show 3 lines above, 1 line below the error line.
    let above = 3usize;
    let below = 1usize;
    let first_line = error_line.saturating_sub(above);
    let last_line = error_line.saturating_add(below).saturating_add(1); // exclusive

    // Compute byte offset of each line start.
    let line_offsets: Vec<usize> = std::iter::once(0)
        .chain(src.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let window_start = line_offsets.get(first_line).copied().unwrap_or(src_len);
    let window_end = line_offsets.get(last_line).copied().unwrap_or(src_len);
    let windowed = &src[window_start..window_end];

    // Rebase span ranges relative to the windowed slice.
    let rebase = |s: Span| -> std::ops::Range<usize> {
        let start = s.offset.saturating_sub(window_start).min(windowed.len());
        let end = (s.offset + s.len)
            .saturating_sub(window_start)
            .min(windowed.len());
        start..end
    };

    let snip = Snippet::source(windowed)
        .path(diag.file.as_str())
        .line_start(first_line + 1)
        .fold(false)
        .annotation(
            AnnotationKind::Primary
                .span(rebase(diag.primary))
                .label(diag.message.as_str()),
        );

    let groups = vec![
        Level::ERROR
            .primary_title(diag.message.as_str())
            .element(snip),
    ];

    let renderer = if is_terminal() {
        Renderer::styled().decor_style(DecorStyle::Ascii)
    } else {
        Renderer::plain().decor_style(DecorStyle::Ascii)
    };

    renderer.render(&groups)
}

/// Print a diagnostic to stderr.
pub fn print_diagnostic(diag: &Diagnostic) {
    anstream::eprint!("{}", render_diagnostic(diag));
}
