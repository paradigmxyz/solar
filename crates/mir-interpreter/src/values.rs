//! Logical SSA products for semantic replay. Fields retain their tree structure and
//! undefined components; no memory address or backend return buffer is assigned.
//! Explicit entry/return boundaries flatten products in declaration order.
//! Type depth and total nodes are bounded before constructing or cloning a value.

use alloy_primitives::U256;
use solar_mir::{MirType, Module};

#[derive(Clone, Debug)]
pub(crate) enum RuntimeValue {
    Word(U256),
    Aggregate(Vec<Self>),
    Undefined,
}

impl RuntimeValue {
    pub(crate) fn nodes(&self) -> u64 {
        match self {
            Self::Aggregate(fields) => 1 + fields.iter().map(Self::nodes).sum::<u64>(),
            _ => 1,
        }
    }

    pub(crate) fn input(
        module: &Module,
        ty: MirType,
        words: &mut impl Iterator<Item = U256>,
    ) -> Result<Self, &'static str> {
        if let Some(bits) = super::machine::scalar_bits(ty) {
            let word = words.next().ok_or("missing flattened MIR argument")?;
            if bits < 256 && word >> bits != U256::ZERO {
                return Err("unclean replay argument");
            }
            Ok(Self::Word(word))
        } else {
            Ok(Self::Aggregate(
                fields(module, ty)?
                    .into_iter()
                    .map(|ty| Self::input(module, ty, words))
                    .collect::<Result<_, _>>()?,
            ))
        }
    }

    pub(crate) fn flatten(
        &self,
        module: &Module,
        ty: MirType,
        words: &mut Vec<U256>,
    ) -> Result<(), &'static str> {
        if super::machine::scalar_bits(ty).is_some() {
            let Self::Word(word) = self else { return Err("undefined MIR return component") };
            words.push(*word);
        } else {
            let types = fields(module, ty)?;
            match self {
                Self::Aggregate(values) if values.len() == types.len() => {
                    for (value, ty) in values.iter().zip(types) {
                        value.flatten(module, ty, words)?;
                    }
                }
                Self::Undefined => {
                    for ty in types {
                        Self::Undefined.flatten(module, ty, words)?;
                    }
                }
                _ => return Err("MIR aggregate return shape mismatch"),
            }
        }
        Ok(())
    }

    /// Exposes typed leaves without reading undefined scalar bits.
    pub(crate) fn leaves<'a>(
        &'a self,
        module: &Module,
        ty: MirType,
        result: &mut Vec<&'a Self>,
    ) -> Result<(), &'static str> {
        if super::machine::scalar_bits(ty).is_some() {
            if matches!(self, Self::Aggregate(_)) {
                return Err("MIR scalar return shape mismatch");
            }
            result.push(self);
        } else {
            let types = fields(module, ty)?;
            match self {
                Self::Aggregate(values) if values.len() == types.len() => {
                    for (value, ty) in values.iter().zip(types) {
                        value.leaves(module, ty, result)?;
                    }
                }
                Self::Undefined => {
                    for ty in types {
                        self.leaves(module, ty, result)?;
                    }
                }
                _ => return Err("MIR aggregate return shape mismatch"),
            }
        }
        Ok(())
    }
}

pub(crate) fn fields(module: &Module, ty: MirType) -> Result<Vec<MirType>, &'static str> {
    match ty {
        MirType::Struct(id) => module
            .struct_types
            .get(id)
            .filter(|ty| ty.fields.len() <= 256)
            .map(|ty| ty.fields.to_vec())
            .ok_or("unknown MIR aggregate type"),
        MirType::Slice(location) => Ok(vec![MirType::Ptr(location), MirType::I256]),
        _ => Err("not a MIR aggregate type"),
    }
}

pub(crate) fn widths(module: &Module, ty: MirType) -> Result<Vec<u32>, &'static str> {
    fn walk(
        module: &Module,
        ty: MirType,
        depth: usize,
        nodes: &mut usize,
        result: &mut Vec<u32>,
    ) -> Result<(), &'static str> {
        *nodes += 1;
        if depth > 32 || *nodes > 256 {
            return Err("MIR aggregate type limit: depth 32, nodes 256");
        }
        if let Some(bits @ 1..=256) = super::machine::scalar_bits(ty) {
            result.push(bits);
        } else {
            for ty in fields(module, ty)? {
                walk(module, ty, depth + 1, nodes, result)?;
            }
        }
        Ok(())
    }
    let mut result = Vec::new();
    walk(module, ty, 0, &mut 0, &mut result)?;
    Ok(result)
}
