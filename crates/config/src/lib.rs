//! Compiler config.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/sulk/main/assets/logo.jpg",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
mod macros;

mod utils;

pub use strum;

str_enum! {
    /// Stop execution after the given compiler stage.
    #[derive(strum::EnumIs)]
    #[strum(serialize_all = "lowercase")]
    pub enum StopAfter {
        Parsing,
    }
}

str_enum! {
    /// Source code language.
    #[derive(Default)]
    #[derive(strum::EnumIs)]
    #[strum(serialize_all = "lowercase")]
    pub enum Language {
        #[default]
        Solidity,
        Yul,
    }
}

str_enum! {
    /// A version specifier of the EVM we want to compile to.
    ///
    /// Defaults to the latest version deployed on Ethereum Mainnet at the time of compiler release.
    #[derive(Default)]
    #[strum(serialize_all = "camelCase")]
    pub enum EvmVersion {
        // NOTE: Order matters.
        Homestead,
        TangerineWhistle,
        SpuriousDragon,
        Byzantium,
        Constantinople,
        Petersburg,
        Istanbul,
        Berlin,
        London,
        Paris,
        #[default]
        Shanghai,
        Cancun,
    }
}

impl EvmVersion {
    pub fn supports_returndata(self) -> bool {
        self >= Self::Byzantium
    }
    pub fn has_static_call(self) -> bool {
        self >= Self::Byzantium
    }
    pub fn has_bitwise_shifting(self) -> bool {
        self >= Self::Constantinople
    }
    pub fn has_create2(self) -> bool {
        self >= Self::Constantinople
    }
    pub fn has_ext_code_hash(self) -> bool {
        self >= Self::Constantinople
    }
    pub fn has_chain_id(self) -> bool {
        self >= Self::Istanbul
    }
    pub fn has_self_balance(self) -> bool {
        self >= Self::Istanbul
    }
    pub fn has_base_fee(self) -> bool {
        self >= Self::London
    }
    pub fn has_blob_base_fee(self) -> bool {
        self >= Self::Cancun
    }
    pub fn has_prev_randao(self) -> bool {
        self >= Self::Paris
    }
    pub fn has_push0(self) -> bool {
        self >= Self::Shanghai
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[cfg(not(feature = "serde"))]
    use serde_json as _;

    #[test]
    fn string_enum() {
        for value in EvmVersion::iter() {
            let s = value.to_str();
            assert_eq!(value.to_string(), s);
            assert_eq!(value, s.parse().unwrap());
            #[cfg(feature = "serde")]
            {
                let json_s = format!("\"{value}\"");
                assert_eq!(serde_json::to_string(&value).unwrap(), json_s);
                assert_eq!(serde_json::from_str::<EvmVersion>(&json_s).unwrap(), value);
            }
        }
    }
}
