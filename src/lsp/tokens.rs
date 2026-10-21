//! Semantic tokens at the protocol boundary: the legend the compiler declares
//! and the delta encoding the protocol carries.

use tower_lsp_server::ls_types::{
    SemanticToken as ProtocolToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensLegend,
};

use crate::compiler::{SemanticToken, TOKEN_MODIFIERS, TOKEN_TYPES};

use super::positions::{Encoding, offset_to_position};

/// The legend the compiler declares: one token type per role, in the order the
/// roles enumerate, and the one modifier a declaration carries.
pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES
            .iter()
            .map(|name| SemanticTokenType::new(name))
            .collect(),
        token_modifiers: TOKEN_MODIFIERS
            .iter()
            .map(|name| SemanticTokenModifier::new(name))
            .collect(),
    }
}

/// The tokens as the protocol carries them: five integers per token, each
/// position relative to the token before it. A token is clipped to its line:
/// the protocol carries one line per token, so a span that crosses a line ends
/// where its line ends.
pub fn encode(source: &str, tokens: Vec<SemanticToken>, encoding: Encoding) -> SemanticTokens {
    let mut data = Vec::with_capacity(tokens.len() * 5);
    let mut previous = (0u32, 0u32);
    for token in tokens {
        let start = offset_to_position(source, token.span.start, encoding);
        let end = offset_to_position(source, token.span.end, encoding);
        let end_character = if start.line == end.line {
            end.character
        } else {
            line_end_character(source, token.span.start, encoding)
        };
        if end_character <= start.character {
            continue;
        }
        let delta_line = start.line - previous.0;
        let delta_start = if delta_line == 0 {
            start.character - previous.1
        } else {
            start.character
        };
        previous = (start.line, start.character);
        data.push(ProtocolToken {
            delta_line,
            delta_start,
            length: end_character - start.character,
            token_type: token.role.slot() as u32,
            token_modifiers_bitset: u32::from(token.declaration),
        });
    }
    SemanticTokens {
        result_id: None,
        data,
    }
}

/// The code-unit column the line holding `offset` ends at.
fn line_end_character(source: &str, offset: usize, encoding: Encoding) -> u32 {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |index| line_start + index);
    offset_to_position(source, line_end, encoding).character
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::SemanticRole;
    use crate::syntax::Span;

    /// The five fields of every encoded token.
    fn fields(encoded: &SemanticTokens) -> Vec<(u32, u32, u32, u32, u32)> {
        encoded
            .data
            .iter()
            .map(|token| {
                (
                    token.delta_line,
                    token.delta_start,
                    token.length,
                    token.token_type,
                    token.token_modifiers_bitset,
                )
            })
            .collect()
    }

    /// One token at `span` playing `role`.
    fn token(span: Span, role: SemanticRole, declaration: bool) -> SemanticToken {
        SemanticToken {
            span,
            role,
            declaration,
        }
    }

    #[test]
    fn the_tokens_are_relative_to_the_one_before_them() {
        let source = "fn main() {\n    let value: int = 1;\n};\n";
        let tokens = vec![
            token(Span::new(0, 2), SemanticRole::Keyword, false),
            token(Span::new(3, 7), SemanticRole::Function, true),
            token(Span::new(16, 19), SemanticRole::Keyword, false),
            token(Span::new(20, 25), SemanticRole::Variable, true),
        ];
        let encoded = encode(source, tokens, Encoding::Utf8);
        assert_eq!(
            fields(&encoded),
            vec![
                (0, 0, 2, 8, 0), // fn
                (0, 3, 4, 2, 1), // main
                (1, 4, 3, 8, 0), // let
                (0, 4, 5, 6, 1), // value
            ]
        );
    }

    #[test]
    fn a_token_crossing_a_line_is_clipped_to_its_line() {
        let source = "a \"two\nlines\" b";
        let tokens = vec![token(Span::new(2, 12), SemanticRole::String, false)];
        let encoded = encode(source, tokens, Encoding::Utf8);
        assert_eq!(
            fields(&encoded),
            vec![(0, 2, 4, 9, 0)],
            "the string stops at the end of its line"
        );
    }

    #[test]
    fn the_legend_names_every_role_once() {
        let legend = legend();
        assert_eq!(legend.token_types.len(), TOKEN_TYPES.len());
        assert_eq!(
            legend.token_types[SemanticRole::EnumMember.slot()].as_str(),
            "enumMember"
        );
        assert_eq!(legend.token_modifiers[0].as_str(), "declaration");
    }
}
