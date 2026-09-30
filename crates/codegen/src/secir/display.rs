//! Textual SecIR dump.

use super::{
    Branch, CallKind, ContractFacts, EventEmission, ExternalCall, FunctionFacts, FunctionKind,
    Guard, GuardKind, Hazard, HazardKind, InternalCall, SelfDestruct, SlotFlow, Source,
    StorageAccess, StorageSlot,
};
use alloy_primitives::{U256, hex};
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
            if !self.storage_flows.is_empty() {
                writeln!(f, "  storage flows:")?;
                for flow in &self.storage_flows {
                    cx.flow(f, flow)?;
                }
            }
            let value = [
                (self.receives_value, "receives value"),
                (self.sends_value, "sends value"),
                (self.locks_value(), "locks value"),
            ];
            let value = value.iter().filter(|(set, _)| *set).map(|(_, name)| *name);
            let value = value.collect::<Vec<_>>();
            if !value.is_empty() {
                writeln!(f, "  {}", value.join(", "))?;
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

        if !function.modifiers.is_empty() {
            let modifiers = function.modifiers.iter().map(|name| name.to_string());
            writeln!(f, "    modifiers {}", modifiers.collect::<Vec<_>>().join(", "))?;
        }
        if !function.entry_checks.is_empty() {
            let access = if function.is_access_controlled() { " (access control)" } else { "" };
            writeln!(f, "    entry checks {}{access}", self.sources(&function.entry_checks))?;
        }
        if !function.returns.is_empty() {
            writeln!(f, "    returns {}", self.sources(&function.returns))?;
        }
        for access in &function.storage_reads {
            self.access(f, "reads", access)?;
        }
        for access in &function.storage_writes {
            self.access(f, "writes", access)?;
        }
        for call in &function.external_calls {
            self.external_call(f, "", call)?;
        }
        for call in &function.internal_calls {
            self.internal_call(f, call)?;
        }
        for guard in &function.guards {
            self.guard(f, guard)?;
        }
        for branch in &function.branches {
            self.branch(f, branch)?;
        }
        for event in &function.events {
            self.event(f, "", event)?;
        }
        for self_destruct in &function.self_destructs {
            self.self_destruct(f, "", self_destruct)?;
        }
        if !function.constants.is_empty() {
            let constants = function.constants.iter().map(|&value| constant(value));
            writeln!(f, "    constants {}", constants.collect::<Vec<_>>().join(", "))?;
        }
        for &span in &function.unchecked_arithmetic {
            writeln!(f, "    unchecked {}", self.span(Some(span)))?;
        }
        for hazard in &function.hazards {
            self.hazard(f, hazard)?;
        }

        // Effects reached only through internal calls.
        let summary = &function.summary;
        let reads = summary
            .storage_reads
            .iter()
            .filter(|access| {
                !function.storage_reads.iter().any(|direct| direct.slot == access.slot)
            })
            .map(|access| access.slot)
            .collect::<BTreeSet<_>>();
        if !reads.is_empty() {
            let reads = reads.iter().map(|&slot| self.slot(slot));
            writeln!(f, "    transitive reads {}", reads.collect::<Vec<_>>().join(", "))?;
        }
        for access in &summary.storage_writes {
            if !function.storage_writes.contains(access) {
                self.access(f, "transitive writes", access)?;
            }
        }
        for call in &summary.external_calls {
            if !function.external_calls.contains(call) {
                self.external_call(f, "transitive ", call)?;
            }
        }
        for event in &summary.events {
            if !function.events.contains(event) {
                self.event(f, "transitive ", event)?;
            }
        }
        for self_destruct in &summary.self_destructs {
            if !function.self_destructs.contains(self_destruct) {
                self.self_destruct(f, "transitive ", self_destruct)?;
            }
        }

        let reentrancy = summary.reentrancy_slots();
        if !reentrancy.is_empty() {
            let slots = reentrancy.iter().map(|&slot| self.slot(slot));
            writeln!(
                f,
                "    read before and written after external call: {}",
                slots.collect::<Vec<_>>().join(", ")
            )?;
        }
        if matches!(
            function.kind,
            FunctionKind::External | FunctionKind::Fallback | FunctionKind::Receive
        ) && function.writes_without_event()
        {
            writeln!(f, "    writes storage without emitting an event")?;
        }
        Ok(())
    }

    fn flow(&self, f: &mut fmt::Formatter<'_>, flow: &SlotFlow) -> fmt::Result {
        let controlled = if flow.externally_controlled { " [externally controlled]" } else { "" };
        write!(f, "    {}{controlled}:", self.slot(flow.slot))?;
        if !flow.writers.is_empty() {
            let writers = flow.writers.iter().map(|writer| {
                let mut text = self.function_name(writer.function).to_owned();
                if !writer.value.is_empty() {
                    text = format!("{text} from {}", self.sources(&writer.value));
                }
                if writer.guarded {
                    text.push_str(" [guarded]");
                }
                text
            });
            write!(f, " written by {}", writers.collect::<Vec<_>>().join(", "))?;
            if !flow.readers.is_empty() {
                write!(f, ";")?;
            }
        }
        if !flow.readers.is_empty() {
            let readers = flow.readers.iter().map(|&reader| self.function_name(reader));
            write!(f, " read by {}", readers.collect::<Vec<_>>().join(", "))?;
        }
        writeln!(f)
    }

    fn access(
        &self,
        f: &mut fmt::Formatter<'_>,
        label: &str,
        access: &StorageAccess,
    ) -> fmt::Result {
        let transient = if access.transient { "transient " } else { "" };
        write!(f, "    {label} {transient}{}", self.slot(access.slot))?;
        if !access.keys.is_empty() {
            write!(f, " keyed by {}", self.sources(&access.keys))?;
        }
        if !access.value.is_empty() {
            write!(f, " from {}", self.sources(&access.value))?;
        }
        let flags = flags(&[
            (access.guarded, "guarded"),
            (access.in_loop, "in loop"),
            (access.after_external_call, "after call"),
        ]);
        writeln!(f, "{flags} {}", self.span(access.span))
    }

    fn external_call(
        &self,
        f: &mut fmt::Formatter<'_>,
        prefix: &str,
        call: &ExternalCall,
    ) -> fmt::Result {
        write!(f, "    {prefix}{}", call_kind(call.kind))?;
        if let Some(selector) = call.selector {
            write!(f, " 0x{}", hex::encode(selector))?;
        }
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
        if call.args.iter().any(|arg| !arg.is_empty()) {
            write!(f, " args ({})", self.args(&call.args))?;
        }
        let flags = flags(&[
            (call.target_controlled, "controlled target"),
            (call.result_unchecked, "unchecked result"),
            (call.guarded, "guarded"),
            (call.in_loop, "in loop"),
            (call.after_external_call, "after call"),
        ]);
        writeln!(f, "{flags} {}", self.span(call.span))
    }

    fn internal_call(&self, f: &mut fmt::Formatter<'_>, call: &InternalCall) -> fmt::Result {
        let flags = flags(&[
            (call.guarded, "guarded"),
            (call.in_loop, "in loop"),
            (call.after_external_call, "after call"),
        ]);
        let callee = self.function_name(call.callee);
        writeln!(f, "    icall {callee}({}){flags} {}", self.args(&call.args), self.span(call.span))
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

    fn branch(&self, f: &mut fmt::Formatter<'_>, branch: &Branch) -> fmt::Result {
        write!(f, "    branch")?;
        if !branch.sources.is_empty() {
            write!(f, " on {}", self.sources(&branch.sources))?;
        }
        let flags = flags(&[(branch.in_loop, "in loop")]);
        writeln!(f, "{flags} {}", self.span(branch.span))
    }

    fn event(
        &self,
        f: &mut fmt::Formatter<'_>,
        prefix: &str,
        event: &EventEmission,
    ) -> fmt::Result {
        let name = event.name.map_or_else(|| "?".to_owned(), |name| name.to_string());
        let flags = flags(&[
            (event.guarded, "guarded"),
            (event.in_loop, "in loop"),
            (event.after_external_call, "after call"),
        ]);
        writeln!(
            f,
            "    {prefix}emit {name}({}){flags} {}",
            self.args(&event.args),
            self.span(event.span)
        )
    }

    fn self_destruct(
        &self,
        f: &mut fmt::Formatter<'_>,
        prefix: &str,
        self_destruct: &SelfDestruct,
    ) -> fmt::Result {
        write!(f, "    {prefix}selfdestruct")?;
        if !self_destruct.beneficiary.is_empty() {
            write!(f, " to {}", self.sources(&self_destruct.beneficiary))?;
        }
        let flags = flags(&[(self_destruct.guarded, "guarded")]);
        writeln!(f, "{flags} {}", self.span(self_destruct.span))
    }

    fn hazard(&self, f: &mut fmt::Formatter<'_>, hazard: &Hazard) -> fmt::Result {
        let kind = match hazard.kind {
            HazardKind::StrictEquality => "strict equality",
            HazardKind::DivideBeforeMultiply => "divide before multiply",
            HazardKind::WeakRandomness => "weak randomness",
            HazardKind::CallValueInLoop => "msg.value in loop",
        };
        write!(f, "    hazard {kind}")?;
        if !hazard.sources.is_empty() {
            write!(f, " on {}", self.sources(&hazard.sources))?;
        }
        writeln!(f, " {}", self.span(hazard.span))
    }

    fn function_name(&self, index: usize) -> &str {
        self.facts.functions.get(index).map_or("?", |function| &function.name)
    }

    fn args(&self, args: &[BTreeSet<Source>]) -> String {
        let args =
            args.iter().map(|arg| if arg.is_empty() { "_".to_owned() } else { self.sources(arg) });
        args.collect::<Vec<_>>().join("; ")
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
                Source::KeyedByCaller => "[msg.sender]".to_owned(),
                Source::KeyedByArgument(index) => format!("[arg{index}]"),
                Source::Immutable => "immutable".to_owned(),
                Source::CallResult => "call result".to_owned(),
                Source::Calldata => "calldata".to_owned(),
                Source::Timestamp => "block.timestamp".to_owned(),
                Source::BlockNumber => "block.number".to_owned(),
                Source::Randomness => "randomness".to_owned(),
                Source::Balance => "balance".to_owned(),
                Source::Environment => "environment".to_owned(),
                Source::Memory => "memory".to_owned(),
            })
            .collect::<Vec<_>>()
            .join(", ")
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

/// Formats the set flags as ` [a, b]`, or nothing when none is set.
fn flags(flags: &[(bool, &str)]) -> String {
    let set = flags.iter().filter(|(set, _)| *set).map(|(_, name)| *name).collect::<Vec<_>>();
    if set.is_empty() { String::new() } else { format!(" [{}]", set.join(", ")) }
}

/// Formats a constant in decimal when small and in hexadecimal otherwise.
fn constant(value: U256) -> String {
    if value <= U256::from(0xffff) { value.to_string() } else { format!("{value:#x}") }
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
