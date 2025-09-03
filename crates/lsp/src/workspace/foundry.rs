use serde::Deserialize;
use solar_config::{EvmVersion, ImportRemapping};

/// A subset of `foundry.toml` that the LSP will parse
/// using `forge config --json`.
///
/// This will be merged into the main configuration.
#[derive(Debug, Deserialize)]
pub(crate) struct FoundryConfig {
    #[serde(with = "crate::serde::display_fromstr::vec")]
    remappings: Vec<ImportRemapping>,
    #[serde(with = "crate::serde::display_fromstr")]
    evm_version: EvmVersion,
}
