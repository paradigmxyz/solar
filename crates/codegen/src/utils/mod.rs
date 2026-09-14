//! Shared formatting helpers for MIR and EVM IR.

use solar_interface::Symbol;
use std::fmt;

pub(crate) fn display_data_name(name: Symbol, index: usize) -> impl fmt::Display {
    fmt::from_fn(move |f| write!(f, "{name}_{index}"))
}

pub(crate) fn display_data_ref(
    name: Option<Symbol>,
    index: usize,
    offset: u32,
) -> impl fmt::Display {
    fmt::from_fn(move |f| {
        if let Some(name) = name {
            write!(f, "{}", display_data_name(name, index))?;
        } else {
            write!(f, "{index}")?;
        }
        if offset != 0 {
            write!(f, "+{offset}")?;
        }
        Ok(())
    })
}
