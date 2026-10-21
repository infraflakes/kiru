//! The semantic tokens at the protocol boundary: the legend the compiler
//! declares, and the delta encoding the protocol carries.

use tower_lsp_server::ls_types::{
    SemanticToken as ProtocolToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensLegend,
};

use crate::compiler::{SemanticToken, TOKEN_MODIFIERS, TOKEN_TYPES};

use super::positions::{Encoding, offset_to_position};

/// The legend the compiler declares: one token type per role, in the order
/// the roles enumerate, and the modifiers a role may carry.
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

/// The tokens as the protocol carries them: each one relative to the one
/// before it, a definition marked with the legend's second modifier bit.
pub fn encoded(source: &str, tokens: Vec<SemanticToken>, encoding: Encoding) -> SemanticTokens {
    let mut data = Vec::with_capacity(tokens.len());
    let mut previous = (0u32, 0u32);
    for token in tokens {
        let start = offset_to_position(source, token.span.start, encoding);
        let end = offset_to_position(source, token.span.end, encoding);
        // A token that spans lines has no single line to paint: it is the
        // literal's content, and the protocol carries one line's offset.
        if start.line != end.line {
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
            length: end.character - start.character,
            token_type: token.role.slot() as u32,
            token_modifiers_bitset: u32::from(token.definition),
        });
    }
    SemanticTokens {
        result_id: None,
        data,
    }
}
