mod config;
mod parser;
mod runner;

pub(crate) use config::{FlycheckConfig, FlycheckInitializationOptions};
#[cfg(test)]
pub(crate) use parser::SourceSnapshot;
pub(crate) use runner::{FlycheckResult, run};
