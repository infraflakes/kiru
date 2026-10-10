//! The language's type set.
//!
//! The type registry declares every type once: the keyword the source writes,
//! and the name diagnostics use. The macro generates the `Type` enum and the
//! keyword maps together, so a type cannot exist without its keyword and name,
//! and the two can never drift. An internal type such as `Void` declares no
//! keyword, because the source never writes it.

/// Declare every type once. Each entry generates one enum variant and its
/// keyword and diagnostic name. A `keyword` is absent for an internal type the
/// source never writes.
macro_rules! type_registry {
    ($( $variant:ident { $( keyword: $keyword:literal, )? name: $name:literal } ),* $(,)?) => {
        /// A type. The data types are user-facing; an internal type such as
        /// `Void` is the result of a call that hands back no value.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub(crate) enum Type {
            $( $variant ),*
        }

        impl Type {
            /// Every type, in declaration order.
            pub(crate) const ALL: &'static [Type] = &[ $( Type::$variant ),* ];

            /// The name diagnostics use.
            pub(crate) fn name(self) -> &'static str {
                match self {
                    $( Type::$variant => $name ),*
                }
            }

            /// The keyword the source writes, or `None` for an internal type.
            pub(crate) fn keyword(self) -> Option<&'static str> {
                match self {
                    $( Type::$variant => type_registry!(@keyword $($keyword)?) ),*
                }
            }

            /// The type a written keyword names, or `None` when no type uses it.
            pub(crate) fn from_keyword(keyword: &str) -> Option<Type> {
                match keyword {
                    $( $( $keyword => Some(Type::$variant), )? )*
                    _ => None,
                }
            }
        }
    };
    (@keyword $keyword:literal) => { Some($keyword) };
    (@keyword) => { None };
}

type_registry! {
    Text { keyword: "txt", name: "text" },
    Record { keyword: "rec", name: "record" },
    List { keyword: "list", name: "list" },
    Void { name: "void" },
}

/// Whether a value of `actual` may stand where `required` is asked for. Data
/// types are exact, so this is equality; it is the single compatibility point
/// every position, binding, parameter, and return passes through.
pub(crate) fn fits(actual: Type, required: Type) -> bool {
    actual == required
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_has_a_name() {
        for ty in Type::ALL {
            assert!(!ty.name().is_empty(), "{ty:?} has no name");
        }
    }

    #[test]
    fn every_written_keyword_round_trips() {
        for ty in Type::ALL {
            if let Some(keyword) = ty.keyword() {
                assert_eq!(Type::from_keyword(keyword), Some(*ty), "{ty:?}");
            }
        }
    }

    #[test]
    fn a_value_fits_only_its_own_type() {
        for actual in Type::ALL {
            for required in Type::ALL {
                assert_eq!(
                    fits(*actual, *required),
                    actual == required,
                    "{actual:?} against {required:?}"
                );
            }
        }
    }
}
