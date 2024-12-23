macro_rules! str_enum {
    ($(#[$attr:meta])* $vis:vis enum $name:ident { $( $(#[$var_attr:meta])* $var:ident),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(strum::IntoStaticStr, strum::EnumIter, strum::EnumCount, strum::VariantNames)]
        // Use `clap` in `FromStr` for a better error message when available.
        #[cfg_attr(not(feature = "clap"), derive(strum::EnumString))]
        $(#[$attr])*
        $vis enum $name {
            $(
                $(#[$var_attr])*
                $var
            ),*
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.to_str())
            }
        }

        #[cfg(feature = "clap")]
        impl clap::ValueEnum for $name {
            fn value_variants<'a>() -> &'a [Self] {
                &[$(Self::$var),*]
            }

            fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
                Some(clap::builder::PossibleValue::new(self.to_str()))
            }
        }

        #[cfg(feature = "clap")]
        impl std::str::FromStr for $name {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                <Self as clap::ValueEnum>::from_str(s, false).map_err(|e| e.to_string())
            }
        }

        #[cfg(feature = "serde")]
        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.to_str())
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                deserializer.deserialize_any(crate::utils::StrumVisitor::<Self>::new())
            }
        }

        impl $name {
            /// Returns the string representation of `self`.
            pub fn to_str(self) -> &'static str {
                self.into()
            }
        }
    };
}
