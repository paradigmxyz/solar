//! Textual SecIR dump.

use super::{
    CallKind, ContractFacts, ExternalCall, FunctionFacts, FunctionKind, Guard, GuardKind, Source,
    StorageAccess, StorageSlot,
};
use alloy_primitives::hex;
use solar_interface::Span;
use solar_sema::Gcx;
use std::{collections::BTreeSet, fmt};

impl ContractFacts {
    /// Formats the facts as human-readable text, resolving spans to source locations.
    ///
    /// The format is intended for debugging and tests, and is not stable.
    pub fn display<'a, 'gcx>(&'a self, gcx: Gcx<'gcx>) -> impl fmt::Display + use<'a, 'gcx> {
        fmt::from_fn(move |f| {
            let cx = DisplayCx { gcx, facts: self };
            writeln!(f, "contract {}", self.name)?;
            if !self.state_variables.is_empty() {
                writeln!(f, "  storage:")?;
                for variable in &self.state_variables {
                    let name =
                        variable.name.map_or_else(|| "_".to_owned(), |name| name.to_string());
                    let transient = if variable.transient { "transient " } else { "" };
                    write!(f, "    {transient}slot {}", variable.slot)?;
                    if variable.offset != 0 {
                        write!(f, "+{}", variable.offset)?;
                    }
                    writeln!(f, ": {name}")?;
                }
            }
            for function in &self.functions {
                cx.function(f, function)?;
            }
            Ok(())
        })
    }
}

struct DisplayCx<'a, 'gcx> {
    gcx: Gcx<'gcx>,
    facts: &'a ContractFacts,
}

impl DisplayCx<'_, '_> {
    fn function(&self, f: &mut fmt::Formatter<'_>, function: &FunctionFacts) -> fmt::Result {
        let kind = match function.kind {
            FunctionKind::External => "external",
            FunctionKind::Constructor => "constructor",
            FunctionKind::Fallback => "fallback",
            FunctionKind::Receive => "receive",
            FunctionKind::Internal => "internal",
        };
        write!(f, "  fn {} [{kind}", function.name)?;
        if let Some(selector) = function.selector {
            write!(f, " 0x{}", hex::encode(selector))?;
        }
        writeln!(f, " {}] {}", function.state_mutability, self.span(Some(function.span)))?;

        self.accesses(f, "reads", &function.storage_reads)?;
        self.accesses(f, "writes", &function.storage_writes)?;
        for call in &function.external_calls {
            self.external_call(f, call)?;
        }
        for call in &function.internal_calls {
            let callee = self.facts.functions.get(call.callee).map_or("?", |callee| &callee.name);
            writeln!(f, "    icall {callee} {}", self.span(call.span))?;
        }
        for guard in &function.guards {
            self.guard(f, guard)?;
        }
        for &span in &function.unchecked_arithmetic {
            writeln!(f, "    unchecked {}", self.span(Some(span)))?;
        }
        self.accesses(f, "writes after external call", &function.writes_after_external_call)?;
        if function.self_destructs {
            writeln!(f, "    selfdestruct")?;
        }

        let summary = &function.summary;
        let direct_reads = function.storage_reads.iter().map(|access| access.slot).collect();
        let direct_writes = function.storage_writes.iter().map(|access| access.slot).collect();
        let direct_calls = function.external_calls.iter().map(|call| call.kind).collect();
        if summary.storage_reads != direct_reads
            || summary.storage_writes != direct_writes
            || summary.external_calls != direct_calls
            || summary.self_destructs != function.self_destructs
        {
            write!(f, "    transitive:")?;
            if !summary.storage_reads.is_empty() {
                write!(f, " reads {}", self.slots(&summary.storage_reads))?;
            }
            if !summary.storage_writes.is_empty() {
                write!(f, " writes {}", self.slots(&summary.storage_writes))?;
            }
            if !summary.external_calls.is_empty() {
                let calls = summary.external_calls.iter().map(|&kind| call_kind(kind));
                write!(f, " calls {}", calls.collect::<Vec<_>>().join(", "))?;
            }
            if summary.self_destructs {
                write!(f, " selfdestruct")?;
            }
            writeln!(f)?;
        }
        Ok(())
    }

    fn accesses(
        &self,
        f: &mut fmt::Formatter<'_>,
        label: &str,
        accesses: &[StorageAccess],
    ) -> fmt::Result {
        for access in accesses {
            let transient = if access.transient { "transient " } else { "" };
            write!(f, "    {label} {transient}{}", self.slot(access.slot))?;
            if !access.keys.is_empty() {
                write!(f, " keyed by {}", self.sources(&access.keys))?;
            }
            writeln!(f, " {}", self.span(access.span))?;
        }
        Ok(())
    }

    fn external_call(&self, f: &mut fmt::Formatter<'_>, call: &ExternalCall) -> fmt::Result {
        write!(f, "    {}", call_kind(call.kind))?;
        if !call.target.is_empty() {
            write!(f, " to {}", self.sources(&call.target))?;
        }
        if call.sends_value {
            if call.value.is_empty() {
                write!(f, " with value")?;
            } else {
                write!(f, " with value from {}", self.sources(&call.value))?;
            }
        }
        writeln!(f, " {}", self.span(call.span))
    }

    fn guard(&self, f: &mut fmt::Formatter<'_>, guard: &Guard) -> fmt::Result {
        let kind = match guard.kind {
            GuardKind::Revert => "revert",
            GuardKind::Panic => "panic",
        };
        let access = if guard.is_access_control() { " (access control)" } else { "" };
        write!(f, "    guard {kind}{access}")?;
        if !guard.sources.is_empty() {
            write!(f, " on {}", self.sources(&guard.sources))?;
        }
        writeln!(f, " {}", self.span(guard.span))
    }

    fn sources(&self, sources: &BTreeSet<Source>) -> String {
        sources
            .iter()
            .map(|&source| match source {
                Source::Caller => "msg.sender".to_owned(),
                Source::Origin => "tx.origin".to_owned(),
                Source::CallValue => "msg.value".to_owned(),
                Source::Argument(index) => format!("arg{index}"),
                Source::Storage(slot) => self.slot(slot),
                Source::TransientStorage(slot) => format!("transient {}", self.slot(slot)),
                Source::Immutable => "immutable".to_owned(),
                Source::CallResult => "call result".to_owned(),
                Source::Calldata => "calldata".to_owned(),
                Source::Environment => "environment".to_owned(),
                Source::Memory => "memory".to_owned(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn slots(&self, slots: &BTreeSet<StorageSlot>) -> String {
        slots.iter().map(|&slot| self.slot(slot)).collect::<Vec<_>>().join(", ")
    }

    fn slot(&self, slot: StorageSlot) -> String {
        let name = |slot| {
            let names = self
                .facts
                .state_variables
                .iter()
                .filter(|variable| variable.slot == slot)
                .filter_map(|variable| variable.name)
                .map(|name| name.to_string())
                .collect::<Vec<_>>();
            if names.is_empty() { format!("slot {slot}") } else { names.join("|") }
        };
        match slot {
            StorageSlot::Exact(slot) => name(slot),
            StorageSlot::Derived(slot) => format!("{}[..]", name(slot)),
            StorageSlot::Unknown => "slot ?".to_owned(),
        }
    }

    fn span(&self, span: Option<Span>) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            let Some(span) = span.filter(|span| !span.is_dummy()) else {
                return f.write_str("@ ?");
            };
            let loc = self.gcx.sess.source_map().lookup_char_pos(span.lo());
            write!(f, "@ {}:{}", loc.data.line, loc.data.col.0 + 1)
        })
    }
}

fn call_kind(kind: CallKind) -> &'static str {
    match kind {
        CallKind::Call => "call",
        CallKind::StaticCall => "staticcall",
        CallKind::DelegateCall => "delegatecall",
        CallKind::CallCode => "callcode",
        CallKind::Transfer => "transfer",
        CallKind::Send => "send",
        CallKind::Create => "create",
        CallKind::Create2 => "create2",
    }
}
