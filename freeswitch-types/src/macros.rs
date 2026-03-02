/// Generates a non-exhaustive enum mapping Rust variants to wire-format strings.
///
/// Produces: enum definition + `as_str()` + `Display` + `AsRef<str>` + `FromStr`.
/// The error type must be defined separately (matching existing crate patterns like
/// `ParseEventTypeError`, `ParseChannelStateError`).
///
/// # Example
///
/// ```ignore
/// define_header_enum! {
///     error_type: ParseMyEnumError,
///     /// Doc comment for the enum.
///     pub enum MyEnum {
///         Foo => "foo-wire",
///         Bar => "bar-wire",
///     }
/// }
/// ```
macro_rules! define_header_enum {
    (
        error_type: $Err:ident,
        $(#[$enum_meta:meta])*
        $vis:vis enum $Name:ident {
            $(
                $(#[$var_meta:meta])*
                $variant:ident => $wire:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        #[allow(missing_docs)]
        $vis enum $Name {
            $(
                $(#[$var_meta])*
                $variant,
            )+
        }

        impl $Name {
            /// Wire-format name string.
            pub fn as_str(&self) -> &'static str {
                match self {
                    $( $Name::$variant => $wire, )+
                }
            }
        }

        impl std::fmt::Display for $Name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl AsRef<str> for $Name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl std::str::FromStr for $Name {
            type Err = $Err;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                $(
                    if s.eq_ignore_ascii_case($wire) {
                        return Ok($Name::$variant);
                    }
                )+
                Err($Err(s.to_string()))
            }
        }
    };
}
