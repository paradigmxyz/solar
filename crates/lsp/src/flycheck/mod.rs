mod config;
mod parser;
mod runner;

pub(crate) use config::{FlycheckConfig, FlycheckInitializationOptions};
pub(crate) use runner::run;
