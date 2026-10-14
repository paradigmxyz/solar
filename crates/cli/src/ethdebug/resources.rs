//! Source types and storage recovery recipes for ethdebug resources.
//!
//! Type identity is independent of location. Recursive declarations are interned before their
//! members are visited. Pointer templates use the semantic storage layout, including inherited
//! slots and packing; they never infer an object's layout from optimized machine instructions.

use super::{EthdebugContext, EthdebugSourceRange, make_ethdebug_source_range};
use serde::Serialize;
use serde_json::{Map, Value, json};
use solar_ast::ElementaryType;
use solar_codegen::backend::evm::{DebugStackVariable, DebugWord};
use solar_data_structures::{Never, map::FxHashMap};
use solar_interface::Span;
use solar_sema::{
    Gcx, hir,
    hir::Visit,
    output::{StorageEncoding, StorageLayoutOutput, StorageLayoutType},
    ty::{Ty, TyKind},
};
use std::ops::ControlFlow;

/// A variable's identity and recovery recipe at one machine instruction or across a program.
#[derive(Clone, Debug, Serialize)]
pub(super) struct Variable {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) identifier: Option<String>,
    pub(super) declaration: EthdebugSourceRange,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub(super) ty: Option<Reference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) pointer: Option<Value>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct Reference {
    id: String,
}

#[derive(Clone, Debug, Serialize)]
struct Member {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(rename = "type")]
    ty: Reference,
}

#[derive(Clone, Debug, Serialize)]
struct Definition {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<EthdebugSourceRange>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Type {
    Uint {
        bits: u16,
    },
    Int {
        bits: u16,
    },
    Bool,
    Address {
        payable: bool,
    },
    Bytes {
        #[serde(skip_serializing_if = "Option::is_none")]
        size: Option<u16>,
    },
    String,
    Fixed {
        bits: u16,
        places: u16,
    },
    Ufixed {
        bits: u16,
        places: u16,
    },
    Array {
        contains: Member,
        #[serde(skip_serializing_if = "Option::is_none")]
        count: Option<String>,
    },
    Mapping {
        contains: Mapping,
    },
    Struct {
        contains: Vec<Member>,
        definition: Definition,
    },
    Enum {
        values: Vec<String>,
        definition: Definition,
    },
    Alias {
        contains: Member,
        definition: Definition,
    },
    Tuple {
        contains: Vec<Member>,
    },
    Contract {
        library: bool,
        interface: bool,
        definition: Definition,
    },
    Function {
        external: bool,
        internal: bool,
        contains: Function,
    },
}

#[derive(Clone, Debug, Serialize)]
struct Mapping {
    key: Member,
    value: Member,
}

#[derive(Clone, Debug, Serialize)]
struct Function {
    parameters: Member,
    returns: Member,
}

struct Registry<'gcx> {
    gcx: Gcx<'gcx>,
    sources: FxHashMap<u32, u32>,
    types: Map<String, Value>,
}

impl<'gcx> Registry<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            gcx,
            sources: gcx
                .hir
                .source_ids()
                .map(|id| (gcx.hir.source(id).file.start_pos.0, id.index() as u32))
                .collect(),
            types: Map::new(),
        }
    }

    fn definition(&self, id: impl Into<hir::ItemId>) -> Definition {
        let id = id.into();
        Definition {
            name: self.gcx.item_name(id).to_string(),
            location: make_ethdebug_source_range(
                self.gcx,
                &self.sources,
                self.gcx.hir.item(id).span(),
            ),
        }
    }

    fn member(&mut self, ty: Ty<'gcx>, name: Option<String>) -> Option<Member> {
        Some(Member { name, ty: self.register(ty)? })
    }

    fn register(&mut self, ty: Ty<'gcx>) -> Option<Reference> {
        let ty = ty.peel_refs();
        if let TyKind::Slice(inner) = ty.kind {
            return self.register(inner);
        }
        if !matches!(
            ty.kind,
            TyKind::Elementary(_)
                | TyKind::Array(..)
                | TyKind::DynArray(_)
                | TyKind::Mapping(..)
                | TyKind::Struct(_)
                | TyKind::Enum(_)
                | TyKind::Udvt(..)
                | TyKind::Contract(_)
                | TyKind::Tuple(_)
                | TyKind::Fn(_)
        ) {
            return None;
        }
        let id = self.gcx.debug_type_identifier(ty);
        if self.types.contains_key(&id) {
            return Some(Reference { id });
        }
        // Reserve the identity before walking recursive struct/mapping members.
        self.types.insert(id.clone(), Value::Null);
        let document = self.document(ty);
        let Some(document) = document else {
            self.types.remove(&id);
            return None;
        };
        self.types
            .insert(id.clone(), serde_json::to_value(document).expect("serialize debug type"));
        Some(Reference { id })
    }

    fn document(&mut self, ty: Ty<'gcx>) -> Option<Type> {
        Some(match ty.kind {
            TyKind::Elementary(elementary) => match elementary {
                ElementaryType::UInt(bits) => Type::Uint { bits: bits.bits() },
                ElementaryType::Int(bits) => Type::Int { bits: bits.bits() },
                ElementaryType::Bool => Type::Bool,
                ElementaryType::Address(payable) => Type::Address { payable },
                ElementaryType::FixedBytes(size) => {
                    Type::Bytes { size: Some(u16::from(size.bytes())) }
                }
                ElementaryType::Bytes => Type::Bytes { size: None },
                ElementaryType::String => Type::String,
                ElementaryType::Fixed(bits, places) => {
                    Type::Fixed { bits: bits.bits(), places: u16::from(places.get()) }
                }
                ElementaryType::UFixed(bits, places) => {
                    Type::Ufixed { bits: bits.bits(), places: u16::from(places.get()) }
                }
            },
            TyKind::Array(element, count) => {
                Type::Array { contains: self.member(element, None)?, count: Some(hex_value(count)) }
            }
            TyKind::DynArray(element) => {
                Type::Array { contains: self.member(element, None)?, count: None }
            }
            TyKind::Mapping(key, value) => Type::Mapping {
                contains: Mapping {
                    key: self.member(key, None)?,
                    value: self.member(value, None)?,
                },
            },
            TyKind::Struct(id) => {
                let contains = self
                    .gcx
                    .hir
                    .strukt(id)
                    .fields
                    .iter()
                    .map(|&field| {
                        self.member(
                            self.gcx.type_of_item(field.into()),
                            self.gcx.hir.variable(field).name.map(|name| name.to_string()),
                        )
                    })
                    .collect::<Option<_>>()?;
                Type::Struct { contains, definition: self.definition(id) }
            }
            TyKind::Enum(id) => Type::Enum {
                values: self
                    .gcx
                    .hir
                    .enumm(id)
                    .variants
                    .iter()
                    .map(|&variant| {
                        self.gcx.hir.variable(variant).name.expect("enum name").to_string()
                    })
                    .collect(),
                definition: self.definition(id),
            },
            TyKind::Udvt(inner, id) => {
                Type::Alias { contains: self.member(inner, None)?, definition: self.definition(id) }
            }
            TyKind::Contract(id) => {
                let kind = self.gcx.hir.contract(id).kind;
                Type::Contract {
                    library: kind.is_library(),
                    interface: kind.is_interface(),
                    definition: self.definition(id),
                }
            }
            TyKind::Tuple(types) => Type::Tuple {
                contains: types.iter().map(|ty| self.member(*ty, None)).collect::<Option<_>>()?,
            },
            TyKind::Fn(function) => {
                let parameters = self.gcx.mk_ty(TyKind::Tuple(function.parameters));
                let returns = self.gcx.mk_ty(TyKind::Tuple(function.returns));
                Type::Function {
                    external: function.is_external(),
                    internal: !function.is_external(),
                    contains: Function {
                        parameters: self.member(parameters, None)?,
                        returns: self.member(returns, None)?,
                    },
                }
            }
            _ => return None,
        })
    }
}

pub(super) fn types(gcx: Gcx<'_>) -> Map<String, Value> {
    let mut registry = Registry::new(gcx);
    for id in gcx.hir.variable_ids() {
        registry.register(gcx.type_of_item(id.into()));
    }
    registry.types
}

fn identifier(id: &str) -> String {
    id.replace('(', "$_").replace(')', "_$").replace(',', "_$_")
}

fn sum(a: Value, b: Value) -> Value {
    json!({"~wordsized": {"~sum": [a, b]}})
}

fn region(location: &str, name: &str, slot: Value, offset: Value, length: Value) -> Value {
    json!({"location": location, "name": name, "slot": slot, "offset": offset, "length": length})
}

fn component(
    types: &solar_data_structures::map::FxIndexMap<String, StorageLayoutType>,
    id: &str,
    slot: Value,
    offset: u64,
    name: &str,
    location: &str,
) -> Value {
    let ty = &types[id];
    if matches!(ty.encoding, StorageEncoding::Mapping) {
        return region(location, name, slot, json!(0), json!(32));
    }
    if ty.base.is_some() || !ty.members.is_empty() || matches!(ty.encoding, StorageEncoding::Bytes)
    {
        // Nested templates receive an independent slot binding and a prefix for their regions.
        return json!({"define": {"slot": slot}, "in": {"template": identifier(id), "yields": yields(types, id, name)}});
    }
    let bytes = ty.number_of_bytes.parse::<u64>().expect("value type byte size");
    region(location, name, slot, json!(32 - offset - bytes), json!(bytes))
}

fn yields(
    types: &solar_data_structures::map::FxIndexMap<String, StorageLayoutType>,
    id: &str,
    prefix: &str,
) -> Map<String, Value> {
    produced_names(types, id, &mut Default::default())
        .into_iter()
        .map(|name| {
            let replacement = format!("{prefix}-{name}");
            (name, json!(replacement))
        })
        .collect()
}

fn produced_names(
    types: &solar_data_structures::map::FxIndexMap<String, StorageLayoutType>,
    id: &str,
    visiting: &mut solar_data_structures::map::FxHashSet<String>,
) -> Vec<String> {
    if !visiting.insert(id.to_owned()) {
        return Vec::new();
    }
    let ty = &types[id];
    let mut names = Vec::new();
    if !ty.members.is_empty() {
        for member in &ty.members {
            let child = &types[&member.r#type];
            if matches!(child.encoding, StorageEncoding::Mapping) {
                names.push(member.label.clone());
            } else {
                let produced = produced_names(types, &member.r#type, visiting);
                if produced.is_empty() {
                    names.push(member.label.clone());
                } else {
                    names.extend(
                        produced.into_iter().map(|name| format!("{}-{name}", member.label)),
                    );
                }
            }
        }
    } else if matches!(ty.encoding, StorageEncoding::Bytes) {
        names.extend(["length-flag".into(), "long-length".into(), "data".into()]);
    } else if let Some(base) = &ty.base {
        if matches!(ty.encoding, StorageEncoding::DynamicArray) {
            names.push("length".into());
        }
        let child = produced_names(types, base, visiting);
        if child.is_empty() {
            names.push("item".into());
        } else {
            names.extend(child.into_iter().map(|name| format!("item-{name}")));
        }
    }
    visiting.remove(id);
    names
}

fn hex_value(value: alloy_primitives::U256) -> String {
    let bytes = value.to_be_bytes::<32>();
    let first = bytes.iter().position(|byte| *byte != 0).unwrap_or(31);
    format!("0x{}", alloy_primitives::hex::encode(&bytes[first..]))
}

fn template(
    types: &solar_data_structures::map::FxIndexMap<String, StorageLayoutType>,
    ty: &StorageLayoutType,
) -> Option<Value> {
    let slot = json!("slot");
    let (expect, body) = match ty.encoding {
        StorageEncoding::Mapping => {
            let key = &types[ty.key.as_ref()?];
            let key = if matches!(key.encoding, StorageEncoding::Bytes) {
                json!("key")
            } else {
                json!({"~wordsized": "key"})
            };
            let base = json!({"~keccak256": [key, {"~wordsized": "slot"}]});
            (vec!["slot", "key"], component(types, ty.value.as_ref()?, base, 0, "value", "storage"))
        }
        StorageEncoding::Bytes => {
            let flag = region("storage", "length-flag", slot.clone(), json!(31), json!(1));
            let length = json!({"~quotient": [{"~read": "length-flag"}, 2]});
            let short = region("storage", "data", slot.clone(), json!(0), length);
            let long_length = region("storage", "long-length", slot, json!(0), json!(32));
            let remaining = json!({"~difference": ["length", {"~product": ["index", 32]}]});
            let data_slot = sum(json!("start"), json!("index"));
            let data = json!({"if": {"~difference": [remaining, 32]}, "then": region("storage", "data", data_slot.clone(), json!(0), json!(32)), "else": region("storage", "data", data_slot, json!(0), remaining)});
            let chunks = json!({"list": {"count": {"~quotient": [{"~sum": ["length", 31]}, 32]}, "each": "index", "is": data}});
            let long = json!({"group": [long_length, {"define": {"length": {"~quotient": [{"~read": "long-length"}, 2]}}, "in": {"define": {"start": {"~keccak256": [{"~wordsized": "slot"}]}}, "in": chunks}}]});
            (
                vec!["slot"],
                json!({"group": [flag, {"if": {"~remainder": [{"~read": "length-flag"}, 2]}, "then": long, "else": short}]}),
            )
        }
        StorageEncoding::DynamicArray | StorageEncoding::Inplace if ty.base.is_some() => {
            let base_id = ty.base.as_ref()?;
            let base = &types[base_id];
            let bytes = base
                .number_of_bytes
                .parse::<alloy_primitives::U256>()
                .expect("storage type byte size");
            let dynamic = matches!(ty.encoding, StorageEncoding::DynamicArray);
            let start = if dynamic {
                json!({"~keccak256": [{"~wordsized": "slot"}]})
            } else {
                slot.clone()
            };
            let count = if dynamic {
                json!({"~read": "length"})
            } else {
                // The layout's total size rounds up to words; the declared count is in its label.
                json!(hex_value(
                    ty.label
                        .rsplit('[')
                        .next()?
                        .strip_suffix(']')?
                        .parse::<alloy_primitives::U256>()
                        .expect("static array length")
                ))
            };
            let item = if bytes < alloy_primitives::U256::from(32) {
                let bytes = bytes.to::<u64>();
                let pack = 32 / bytes;
                region(
                    "storage",
                    "item",
                    sum(start, json!({"~quotient": ["index", pack]})),
                    json!({"~difference": [32, {"~product": [{"~sum": [{"~remainder": ["index", pack]}, 1]}, bytes]}]}),
                    json!(bytes),
                )
            } else {
                component(
                    types,
                    base_id,
                    sum(
                        start,
                        json!({"~product": ["index", hex_value(bytes.div_ceil(alloy_primitives::U256::from(32)))]}),
                    ),
                    0,
                    "item",
                    "storage",
                )
            };
            let list = json!({"list": {"count": count, "each": "index", "is": item}});
            (
                vec!["slot"],
                if dynamic {
                    json!({"group": [region("storage", "length", slot, json!(0), json!(32)), list]})
                } else {
                    list
                },
            )
        }
        StorageEncoding::Inplace if !ty.members.is_empty() => {
            let members = ty
                .members
                .iter()
                .map(|member| {
                    component(
                        types,
                        &member.r#type,
                        sum(
                            slot.clone(),
                            json!(hex_value(
                                member
                                    .slot
                                    .parse::<alloy_primitives::U256>()
                                    .expect("storage slot")
                            )),
                        ),
                        member.offset,
                        &member.label,
                        "storage",
                    )
                })
                .collect::<Vec<_>>();
            (vec!["slot"], json!({"group": members}))
        }
        _ => return None,
    };
    Some(json!({"expect": expect, "for": body}))
}

pub(super) fn pointers(gcx: Gcx<'_>) -> Map<String, Value> {
    let mut result = Map::new();
    for contract in gcx.hir.contract_ids() {
        let layout = gcx.storage_layout(contract);
        if let Some(types) = layout.types {
            for (id, ty) in &types {
                if let Some(template) = template(&types, ty) {
                    result.insert(identifier(id), template);
                }
            }
        }
    }
    result
}

fn layout_variables(
    gcx: Gcx<'_>,
    layout: StorageLayoutOutput,
    location: &str,
    sources: &FxHashMap<u32, u32>,
) -> Vec<Variable> {
    let mut variables = Vec::new();
    let Some(types) = layout.types else { return variables };
    for entry in layout.storage {
        let Some(id) =
            gcx.hir.variable_ids().find(|id| gcx.hir.global_item_id(*id) as u64 == entry.ast_id)
        else {
            continue;
        };
        let Some(declaration) = make_ethdebug_source_range(gcx, sources, gcx.hir.variable(id).span)
        else {
            continue;
        };
        let slot =
            json!(hex_value(entry.slot.parse::<alloy_primitives::U256>().expect("storage slot")));
        let mut pointer =
            component(&types, &entry.r#type, slot, entry.offset, &entry.label, location);
        if let Some(root) = pointer.as_object_mut() {
            root.remove("name");
            if let Some(reference) = root.get_mut("in").and_then(Value::as_object_mut) {
                reference.remove("yields");
            }
        }
        variables.push(Variable {
            identifier: Some(entry.label),
            declaration,
            ty: Some(Reference { id: identifier(&entry.r#type) }),
            pointer: Some(pointer),
        });
    }
    variables
}

pub(super) fn context(
    gcx: Gcx<'_>,
    contract: hir::ContractId,
    artifact: &solar_codegen::ContractArtifact,
    deployed: bool,
    sources: &FxHashMap<u32, u32>,
) -> Option<EthdebugContext> {
    let mut variables = layout_variables(gcx, gcx.storage_layout(contract), "storage", sources);
    variables.extend(layout_variables(
        gcx,
        gcx.transient_storage_layout(contract),
        "transient",
        sources,
    ));
    let declaration = gcx.hir.contract(contract);
    let bases = if declaration.linearized_bases.is_empty() {
        std::slice::from_ref(&contract)
    } else {
        declaration.linearized_bases
    };
    for &base in bases {
        for id in gcx.hir.contract(base).variables() {
            let variable = gcx.hir.variable(id);
            if !variable.is_constant() && !variable.is_immutable() {
                continue;
            }
            let ty = gcx.type_of_item(id.into());
            let Some(declaration) = make_ethdebug_source_range(gcx, sources, variable.span) else {
                continue;
            };
            let pointer = if deployed {
                artifact.immutable_references.iter().find(|reference| reference.variable_id == Some(id)).and_then(|reference| {
                    let (bytes, left_aligned) = scalar_bytes(gcx, ty)?;
                    let width = u16::from(reference.type_size.bytes());
                    if bytes > width { return None; }
                    Some(json!({"location": "code", "offset": reference.start + usize::from(if left_aligned { 0 } else { width - bytes }), "length": bytes}))
                })
            } else {
                None
            };
            variables.push(Variable {
                identifier: variable.name.map(|name| name.to_string()),
                declaration,
                ty: Some(Reference { id: gcx.debug_type_identifier(ty) }),
                pointer,
            });
        }
    }
    (!variables.is_empty()).then(|| EthdebugContext { variables, ..Default::default() })
}

pub(super) fn variable_context(
    gcx: Gcx<'_>,
    record: &DebugStackVariable,
    sources: &FxHashMap<u32, u32>,
) -> Option<EthdebugContext> {
    let variable = stack_variable(gcx, record, sources)?;
    let scope = lexical_scope(gcx, record.variable);
    if !scope.contains(record.source) {
        return None;
    }
    let mut context = EthdebugContext {
        name: Some(format!("scope:{}:{}", scope.lo().0, scope.hi().0)),
        code: make_ethdebug_source_range(gcx, sources, scope),
        variables: vec![variable],
        ..Default::default()
    };
    for frame in record.inline.iter().rev() {
        context = EthdebugContext {
            name: Some(format!(
                "inline:{}:{}:{}",
                frame.function.declaration.lo().0,
                frame.call_site.lo().0,
                frame.instance
            )),
            gather: vec![
                EthdebugContext {
                    code: make_ethdebug_source_range(gcx, sources, frame.call_site),
                    ..Default::default()
                },
                context,
            ],
            transform: vec!["inline"],
            ..Default::default()
        };
    }
    Some(context)
}

fn stack_variable(
    gcx: Gcx<'_>,
    record: &DebugStackVariable,
    sources: &FxHashMap<u32, u32>,
) -> Option<Variable> {
    let id = record.variable;
    let variable = gcx.hir.variable(id);
    variable.name?;
    let ty = gcx.type_of_item(id.into());
    let pointer = if let Some(head) = record.abi_head.filter(|_| {
        ty.is_ref_at(solar_ast::DataLocation::Memory)
            || ty.is_ref_at(solar_ast::DataLocation::Calldata)
    }) {
        let address = if ty.is_dynamically_encoded(gcx) {
            json!({"~sum": [4, {"~read": "-carrier-abi-offset"}]})
        } else {
            json!(head)
        };
        let body = object_pointer(gcx, ty, address, None, "calldata", "", 0)?;
        if ty.is_dynamically_encoded(gcx) {
            Some(
                json!({"group": [{"location": "calldata", "name": "-carrier-abi-offset", "offset": head, "length": 32}, body]}),
            )
        } else {
            Some(body)
        }
    } else if let Some((bytes, left_aligned)) = scalar_bytes(gcx, ty) {
        let word = record.words.first()?;
        word_region(word, "value", if left_aligned { 0 } else { 32 - bytes }, bytes)
    } else {
        let word = record.words.first()?;
        let mut carriers = Vec::new();
        let mut address = word_expression(word, "-carrier-address", &mut carriers)?;
        if record.calldata_bias != 0 {
            address = sum(address, json!(record.calldata_bias));
        }
        let count = record
            .words
            .get(1)
            .and_then(|word| word_expression(word, "-carrier-count", &mut carriers));
        let body = if ty.is_ref_at(solar_ast::DataLocation::Storage) {
            let layout = gcx.storage_layout(variable.contract?);
            let types = layout.types?;
            let id = gcx.debug_type_identifier(ty);
            let raw = types.keys().find(|key| identifier(key) == id)?;
            let mut body = component(&types, raw, address, 0, "value", "storage");
            if let Some(root) = body.as_object_mut() {
                root.remove("name");
            }
            body
        } else {
            let location =
                if ty.is_ref_at(solar_ast::DataLocation::Calldata) { "calldata" } else { "memory" };
            object_pointer(gcx, ty, address, count, location, "", 0)?
        };
        carriers.push(body);
        Some(json!({"group": carriers}))
    };
    let mut pointer = pointer;
    if let Some(root) = pointer.as_mut().and_then(Value::as_object_mut) {
        root.remove("name");
    }
    Some(Variable {
        identifier: variable.name.map(|name| name.to_string()),
        declaration: make_ethdebug_source_range(gcx, sources, variable.span)?,
        ty: Some(Reference { id: gcx.debug_type_identifier(ty) }),
        pointer,
    })
}

fn word_region(word: &DebugWord, name: &str, offset: u16, bytes: u16) -> Option<Value> {
    Some(match word {
        DebugWord::Stack(slot) => region("stack", name, json!(slot), json!(offset), json!(bytes)),
        DebugWord::Memory(address) => {
            json!({"location": "memory", "name": name, "offset": hex_value(*address + alloy_primitives::U256::from(offset)), "length": bytes})
        }
        DebugWord::Calldata(address) => {
            json!({"location": "calldata", "name": name, "offset": hex_value(*address + alloy_primitives::U256::from(offset)), "length": bytes})
        }
        // NOTE: The pointer schema has no literal-value region. Keep its declaration/type;
        // constants can still serve as address or length expressions for composite recipes.
        _ => return None,
    })
}

fn word_expression(word: &DebugWord, name: &str, carriers: &mut Vec<Value>) -> Option<Value> {
    if let DebugWord::Constant(value) = word {
        return Some(json!(hex_value(*value)));
    }
    let operator = match word {
        DebugWord::Add(..) => Some("~sum"),
        DebugWord::Sub(..) => Some("~difference"),
        DebugWord::Mul(..) => Some("~product"),
        _ => None,
    };
    if let Some(operator) = operator
        && let DebugWord::Add(lhs, rhs) | DebugWord::Sub(lhs, rhs) | DebugWord::Mul(lhs, rhs) = word
    {
        let lhs = word_expression(lhs, &format!("{name}-lhs"), carriers)?;
        let rhs = word_expression(rhs, &format!("{name}-rhs"), carriers)?;
        return Some(json!({"~wordsized": {operator: [lhs, rhs]}}));
    }
    carriers.push(word_region(word, name, 0, 32)?);
    Some(json!({"~read": name}))
}

/// Uses the source representation: memory objects have word-sized fields and reference elements,
/// while calldata fields have ABI head widths and relative dynamic offsets.
fn object_pointer<'gcx>(
    gcx: Gcx<'gcx>,
    ty: Ty<'gcx>,
    address: Value,
    count: Option<Value>,
    location: &str,
    prefix: &str,
    depth: usize,
) -> Option<Value> {
    // NOTE: Bound recursive type expansion without inventing a region for a recursive tail.
    if depth >= 16 {
        return None;
    }
    let name = |component: &str| {
        if prefix.is_empty() { component.to_owned() } else { format!("{prefix}-{component}") }
    };
    let member = |ty, head: Value, base: Value, member_name: &str| -> Option<Value> {
        if let Some((bytes, left)) = scalar_bytes(gcx, ty) {
            return Some(
                json!({"location": location, "name": member_name, "offset": sum(head, json!(if left { 0 } else { 32 - bytes })), "length": bytes}),
            );
        }
        let mut carriers = Vec::new();
        let dynamic = location == "memory" || ty.is_dynamically_encoded(gcx);
        let target = if dynamic {
            let address_name = format!("-carrier-{member_name}-address");
            carriers.push(
                json!({"location": location, "name": address_name, "offset": head, "length": 32}),
            );
            let read = json!({"~read": address_name});
            if location == "memory" { read } else { sum(base, read) }
        } else {
            head
        };
        carriers.push(object_pointer(gcx, ty, target, None, location, member_name, depth + 1)?);
        Some(json!({"group": carriers}))
    };
    Some(match ty.peel_refs().kind {
        TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
            let data_name = name("data");
            let object = if let Some(count) = count {
                json!({"location": location, "name": data_name, "offset": address, "length": count})
            } else {
                let length_name = name("length");
                json!({"group": [{"location": location, "name": length_name, "offset": address, "length": 32}, {"location": location, "name": data_name, "offset": sum(address.clone(), json!(32)), "length": {"~read": length_name}}]})
            };
            if location == "memory" {
                json!({"if": address, "then": object, "else": {"location": "memory", "name": data_name, "offset": 0, "length": 0}})
            } else {
                object
            }
        }
        TyKind::Struct(id) => {
            let mut head = alloy_primitives::U256::ZERO;
            let mut fields = Vec::new();
            for &field in gcx.hir.strukt(id).fields {
                let variable = gcx.hir.variable(field);
                let field_ty = gcx.type_of_item(field.into());
                fields.push(member(
                    field_ty,
                    sum(address.clone(), json!(hex_value(head))),
                    address.clone(),
                    &name(&variable.name?.to_string()),
                )?);
                head += if location == "memory" {
                    alloy_primitives::U256::from(32)
                } else {
                    field_ty.abi_static_size(gcx).unwrap_or(alloy_primitives::U256::from(32))
                };
            }
            json!({"group": fields})
        }
        TyKind::Array(element, _) | TyKind::DynArray(element) | TyKind::Slice(element) => {
            let fixed = match ty.peel_refs().kind {
                TyKind::Array(_, fixed) => Some(fixed),
                _ => None,
            };
            let null = (location == "memory" && fixed.is_none() && count.is_none())
                .then(|| address.clone());
            let mut regions = Vec::new();
            let (data, length) = if let Some(fixed) = fixed {
                (address, json!(hex_value(fixed)))
            } else if let Some(count) = count {
                (address, count)
            } else {
                let length_name = name("length");
                regions.push(json!({"location": location, "name": length_name, "offset": address, "length": 32}));
                (sum(address, json!(32)), json!({"~read": length_name}))
            };
            let stride = if location == "memory" {
                alloy_primitives::U256::from(32)
            } else {
                element.abi_static_size(gcx).unwrap_or(alloy_primitives::U256::from(32))
            };
            let head = sum(data.clone(), json!({"~product": ["index", hex_value(stride)]}));
            regions.push(json!({"list": {"count": length, "each": "index", "is": member(element, head, data, &name("item"))?}}));
            let body = json!({"group": regions});
            if let Some(address) = null {
                json!({"if": address, "then": body, "else": {"list": {"count": 0, "each": "index", "is": {"name": name("item"), "location": "memory", "offset": 0, "length": 0}}}})
            } else {
                body
            }
        }
        _ => return None,
    })
}

fn lexical_scope(gcx: Gcx<'_>, id: hir::VariableId) -> Span {
    let variable = gcx.hir.variable(id);
    let Some(hir::ItemId::Function(function)) = variable.parent else { return variable.span };
    let function = gcx.hir.function(function);
    let mut finder =
        ScopeFinder { hir: &gcx.hir, declaration: variable.span, scope: function.span };
    let _ = finder.visit_function(function);
    finder.scope
}

struct ScopeFinder<'hir> {
    hir: &'hir hir::Hir<'hir>,
    declaration: Span,
    scope: Span,
}

impl<'hir> Visit<'hir> for ScopeFinder<'hir> {
    type BreakValue = Never;
    fn hir(&self) -> &'hir hir::Hir<'hir> {
        self.hir
    }
    fn visit_stmt(&mut self, stmt: &'hir hir::Stmt<'hir>) -> ControlFlow<Never> {
        let span = match &stmt.kind {
            hir::StmtKind::Block(block)
            | hir::StmtKind::UncheckedBlock(block)
            | hir::StmtKind::AssemblyBlock(block) => Some(block.span),
            hir::StmtKind::Loop(..) => Some(stmt.span),
            hir::StmtKind::Try(try_) => try_
                .clauses
                .iter()
                .find(|clause| clause.span.contains(self.declaration))
                .map(|clause| clause.span),
            _ => None,
        };
        if let Some(span) = span
            && span.contains(self.declaration)
            && self.scope.contains(span)
        {
            self.scope = span;
        }
        self.walk_stmt(stmt)
    }
}

fn scalar_bytes(gcx: Gcx<'_>, ty: Ty<'_>) -> Option<(u16, bool)> {
    Some(match ty.peel_refs().kind {
        TyKind::Elementary(
            ElementaryType::UInt(size)
            | ElementaryType::Int(size)
            | ElementaryType::Fixed(size, _)
            | ElementaryType::UFixed(size, _),
        ) => (u16::from(size.bytes()), false),
        TyKind::Elementary(ElementaryType::Bool) => (1, false),
        TyKind::Elementary(ElementaryType::Address(_)) | TyKind::Contract(_) => (20, false),
        TyKind::Elementary(ElementaryType::FixedBytes(size)) => (u16::from(size.bytes()), true),
        TyKind::Enum(id) => (
            (usize::BITS - gcx.hir.enumm(id).variants.len().saturating_sub(1).leading_zeros())
                .max(1)
                .div_ceil(8) as u16,
            false,
        ),
        TyKind::Udvt(inner, _) => return scalar_bytes(gcx, inner),
        _ => return None,
    })
}
