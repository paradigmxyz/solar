//! The main entry point for the Solar compiler.

#![allow(unused_crate_dependencies)]

use solar_cli::utils;

#[global_allocator]
static ALLOC: utils::Allocator = utils::new_allocator();

pub use solar_cli::main;
