//! Shared formatting helpers for MIR and EVM IR.

use crate::link::DataSize;
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

/// Displays a size derived from a data length: `data[, addend[, aligned]]`.
pub(crate) fn display_data_size(name: Option<Symbol>, size: DataSize) -> impl fmt::Display {
    fmt::from_fn(move |f| {
        write!(f, "{}", display_data_ref(name, size.data.index(), 0))?;
        if size.addend != 0 || size.aligned {
            write!(f, ", {}", size.addend)?;
        }
        if size.aligned {
            f.write_str(", aligned")?;
        }
        Ok(())
    })
}
