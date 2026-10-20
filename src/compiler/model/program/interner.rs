//! The one interner the compiler keys its name tables by.
//!
//! Every table that keys on a name keys on a `Symbol`, and every distinct
//! name becomes exactly one `Symbol` here, whatever pass or table asks for
//! it. A symbol resolves back to its spelling through this interner, so no
//! table carries its own copy of a name.

use std::collections::HashMap;

/// The identity of one interned name, indexing the interner's name table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Symbol(pub(crate) u32);

/// FNV-1a over a byte string: the one fixed hash a name is keyed by. Fixed
/// because the interner must map the same name to the same symbol whatever
/// process runs: the standard's 64 bit parameters, applied with wrapping
/// arithmetic that cannot trap.
pub(crate) fn name_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The names the compiler interned, each once. The map keys a name by its
/// fixed hash; a colliding hash puts both names in one bucket and the bucket
/// is read for the exact spelling, so distinct names stay distinct symbols.
#[derive(Debug, Default)]
pub(crate) struct Interner {
    names: Vec<String>,
    map: HashMap<u64, Vec<u32>>,
}

impl Interner {
    /// The name's symbol, interning it when this is its first appearance.
    pub(crate) fn intern(&mut self, name: &str) -> Symbol {
        let hash = name_hash(name.as_bytes());
        if let Some(symbols) = self.map.get(&hash)
            && let Some(&found) = symbols
                .iter()
                .find(|&&symbol| self.names[symbol as usize] == name)
        {
            return Symbol(found);
        }
        let symbol = self.names.len() as u32;
        self.names.push(name.to_owned());
        self.map.entry(hash).or_default().push(symbol);
        Symbol(symbol)
    }

    /// The name's symbol when the compiler interned it before. A name that
    /// was never interned keys no table, so every table read resolves the
    /// name here first and none resolves on `None`.
    pub(crate) fn resolve(&self, name: &str) -> Option<Symbol> {
        self.map
            .get(&name_hash(name.as_bytes()))?
            .iter()
            .find(|&&symbol| self.names[symbol as usize] == name)
            .map(|&symbol| Symbol(symbol))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_interns_to_one_symbol() {
        let mut interner = Interner::default();
        let first = interner.intern("double");
        assert_eq!(interner.intern("double"), first);
        assert_eq!(
            interner.intern("single"),
            interner.resolve("single").expect("interned")
        );
        assert_ne!(interner.resolve("single"), interner.resolve("double"));
        assert_eq!(interner.resolve("untouched"), None);
    }

    /// Totality over many names: every distinct name gets a distinct symbol,
    /// whatever its spelling, and every table read resolves it again.
    #[test]
    fn every_distinct_name_gets_a_distinct_symbol() {
        let mut interner = Interner::default();
        for index in 0..10_000 {
            let name = format!("name{index}");
            let symbol = interner.intern(&name);
            assert_eq!(
                symbol.0, index as u32,
                "{name} interns in first-contact order"
            );
            assert_eq!(interner.resolve(&name), Some(symbol), "{name} resolves");
            assert_ne!(
                interner.resolve(&format!("other{index}")),
                Some(symbol),
                "a different name holds a different symbol"
            );
        }
    }
}
