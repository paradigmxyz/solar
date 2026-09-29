//! `@custom:solar-safe`: the safe profile of a contract or a library.
//!
//! The tag turns properties of all the code a contract runs in its own call frame into
//! requirements. `memory` requires that none of it is inline assembly, the only way Solidity code
//! reaches memory outside the objects it allocates. `arithmetic` requires that all of its
//! arithmetic is checked: no `unchecked` block, no call to the wrapping operations of
//! `solar:core/v1/Math.sol`, and no inline assembly either, whose arithmetic wraps. A tag that
//! names neither requires both. Other compilers read the tag as documentation.
//!
//! The code a contract runs is what its creation and its entry points reach through internal
//! calls, as the call graph resolves them for the contract: across its bases, libraries and free
//! functions, through modifiers and base constructors, to the override a virtual call dispatches
//! to, and to every function whose value is taken, which a function pointer may call. Its creation
//! also runs, outside any function, the initializers of its bases' state variables and the
//! arguments of their inheritance specifiers. External calls, calls to deployed libraries and
//! contract creations run in call frames of their own, with memory of their own, so they are not
//! part of it. A library's code is all of its functions.
//!
//! Two kinds of code are trusted. The compiler-owned `solar:core/` modules are the primitive layer
//! the profile builds on, so their bodies are not checked. And `@custom:solar-trusted` marks a
//! function, a modifier, a contract or a library as reviewed: the profile does not look inside it,
//! so what only it reaches is not checked either. It is a boundary that review, not this compiler,
//! answers for. An assembly block that only points storage references at their ERC-7201
//! namespaces, which the namespace check verifies, needs no trust.

use super::erc7201::is_namespace_accessor;
use crate::{
    core::is_core_file,
    hir::{self, StmtKind, Visit},
    ty::{Gcx, traced_functions},
};
use solar_data_structures::{
    Never,
    map::{FxHashSet, FxIndexMap},
};
use solar_interface::{Span, source_map::FileName, sym};
use std::ops::ControlFlow;

/// The properties a `@custom:solar-safe` tag requires.
#[derive(Clone, Copy)]
struct Profile {
    memory: bool,
    arithmetic: bool,
    /// The tag.
    tag: Span,
}

pub(super) fn check(gcx: Gcx<'_>) {
    for id in gcx.hir.contract_ids() {
        if let Some(profile) = profile(gcx, id) {
            check_contract(gcx, id, profile);
        }
    }
}

/// The `@custom:solar-safe` tag on the contract `id`, if it has one.
fn profile(gcx: Gcx<'_>, id: hir::ContractId) -> Option<Profile> {
    let contract = gcx.hir.contract(id);
    // The tag on an interface is reported with the documentation.
    if contract.kind == hir::ContractKind::Interface {
        return None;
    }
    let natspec = gcx
        .hir
        .doc(contract.doc)
        .ast_comments
        .iter()
        .flat_map(|comment| comment.natspec.iter())
        .find(|natspec| is_tag(natspec, sym::solar_dash_safe))?;
    let (mut memory, mut arithmetic) = (false, false);
    for property in natspec.content().split_whitespace() {
        match property {
            "memory" => memory = true,
            "arithmetic" => arithmetic = true,
            _ => {
                gcx.dcx()
                    .err(format!("`@custom:solar-safe` has no property `{property}`"))
                    .span(natspec.span)
                    .help("the properties are `memory` and `arithmetic`; naming none requires both")
                    .emit();
            }
        }
    }
    if !memory && !arithmetic {
        (memory, arithmetic) = (true, true);
    }
    Some(Profile { memory, arithmetic, tag: natspec.span })
}

/// Whether `natspec` is the custom tag `name`.
fn is_tag(natspec: &solar_ast::NatSpecItem, name: solar_interface::Symbol) -> bool {
    matches!(natspec.kind, solar_ast::NatSpecKind::Custom { name: tag } if tag.name == name)
}

/// Whether the function `id` is trusted: declared in a compiler-owned module, or reviewed, tagged
/// `@custom:solar-trusted` itself or in a contract tagged so.
fn is_trusted(gcx: Gcx<'_>, id: hir::FunctionId) -> bool {
    let function = gcx.hir.function(id);
    if is_core_file(&gcx.hir.source(function.source).file.name) {
        return true;
    }
    is_tagged_trusted(gcx, function.doc)
        || function.contract.is_some_and(|contract| is_trusted_contract(gcx, contract))
}

/// Whether the contract `id` is trusted: declared in a compiler-owned module, or tagged
/// `@custom:solar-trusted`.
fn is_trusted_contract(gcx: Gcx<'_>, id: hir::ContractId) -> bool {
    let contract = gcx.hir.contract(id);
    is_core_file(&gcx.hir.source(contract.source).file.name) || is_tagged_trusted(gcx, contract.doc)
}

/// Whether the documentation `doc` has the tag `@custom:solar-trusted`.
fn is_tagged_trusted(gcx: Gcx<'_>, doc: hir::DocId) -> bool {
    gcx.hir
        .doc(doc)
        .ast_comments
        .iter()
        .flat_map(|comment| comment.natspec.iter())
        .any(|natspec| is_tag(natspec, sym::solar_dash_trusted))
}

/// Checks the code the contract `id` runs against its profile.
fn check_contract(gcx: Gcx<'_>, id: hir::ContractId, profile: Profile) {
    let findings = findings(gcx, id);
    let name = gcx.hir.contract(id).name;
    for finding in &findings.violations {
        let applies = match finding.violation {
            Violation::Assembly => profile.memory || profile.arithmetic,
            Violation::Unchecked | Violation::Wrapping => profile.arithmetic,
        };
        if !applies {
            continue;
        }
        let what = match finding.violation {
            Violation::Assembly => "inline assembly",
            Violation::Unchecked => "an `unchecked` block",
            Violation::Wrapping => "wrapping arithmetic",
        };
        gcx.dcx()
            .err(format!("`{name}` is tagged `@custom:solar-safe` but runs {what}"))
            .span(finding.span)
            .span_note(profile.tag, "the tag is here")
            .note(match finding.function {
                Some(function) => format!("it runs this through {}", findings.path(gcx, function)),
                None => "it runs this when it is created".to_string(),
            })
            .help(match (finding.violation, finding.function) {
                (Violation::Assembly, _) => {
                    "write it without assembly, or review it and tag its function \
                     `@custom:solar-trusted`"
                }
                (Violation::Unchecked | Violation::Wrapping, Some(_)) => {
                    "use checked arithmetic, or review it and tag its function \
                     `@custom:solar-trusted`"
                }
                // Code outside any function is trusted with the contract that declares it.
                (Violation::Unchecked | Violation::Wrapping, None) => {
                    "use checked arithmetic, or review it and tag its contract \
                     `@custom:solar-trusted`"
                }
            })
            .emit();
    }
}

/// What the code a contract runs does that a safe profile rejects, and the reviewed code it runs.
pub(crate) struct Findings {
    /// What the profile rejects, in the order the code is reached.
    pub(crate) violations: Vec<Finding>,
    /// The functions tagged `@custom:solar-trusted`, or in a contract tagged so, that the code
    /// runs, which the profile does not look inside.
    pub(crate) trusted: Vec<hir::FunctionId>,
    /// The function that first reached each function the code runs, or `None` for a root.
    reached: FxIndexMap<hir::FunctionId, Option<hir::FunctionId>>,
}

/// Something a safe profile rejects.
pub(crate) struct Finding {
    pub(crate) violation: Violation,
    pub(crate) span: Span,
    /// The function whose body it is in, or `None` for the creation code outside any function.
    pub(crate) function: Option<hir::FunctionId>,
}

/// What a safe profile rejects.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Violation {
    /// Inline assembly, which breaks both properties.
    Assembly,
    /// An `unchecked` block.
    Unchecked,
    /// A call to a wrapping operation of `solar:core/v1/Math.sol`.
    Wrapping,
}

impl Findings {
    /// The chain of functions through which the code reaches `function`.
    fn path(&self, gcx: Gcx<'_>, function: hir::FunctionId) -> String {
        describe_path(gcx, &self.reached, function)
    }
}

/// The chain of functions through which a trace that records the function first reaching each
/// function in `reached` reaches `function`, from its root.
pub(super) fn describe_path(
    gcx: Gcx<'_>,
    reached: &FxIndexMap<hir::FunctionId, Option<hir::FunctionId>>,
    function: hir::FunctionId,
) -> String {
    let mut names = Vec::new();
    let mut current = Some(function);
    while let Some(id) = current {
        if names.len() == 8 {
            names.push("…".to_string());
            break;
        }
        // A constructor, a fallback or a receive function is named by its kind.
        let f = gcx.hir.function(id);
        let name = f.name_or_kind();
        names.push(match f.contract {
            Some(contract) => format!("`{}.{name}`", gcx.hir.contract(contract).name),
            None => format!("`{name}`"),
        });
        current = reached.get(&id).copied().flatten();
    }
    names.reverse();
    names.join(" → ")
}

/// Finds what the code the contract `id` runs does that a safe profile rejects.
pub(crate) fn findings(gcx: Gcx<'_>, id: hir::ContractId) -> Findings {
    let library = gcx.hir.contract(id).kind == hir::ContractKind::Library;
    let reached = traced_functions(gcx, id, library, &|function| is_trusted(gcx, function));
    let mut scan = Scan { gcx, current: None, violations: Vec::new(), seen: FxHashSet::default() };
    // The creation runs these outside any function, as the call graph traces them.
    for &base in gcx.hir.contract(id).linearized_bases.iter().rev() {
        if is_trusted_contract(gcx, base) {
            continue;
        }
        let base = gcx.hir.contract(base);
        for variable in base.variables() {
            let variable = gcx.hir.variable(variable);
            if variable.is_state_variable()
                && !variable.is_constant()
                && let Some(initializer) = variable.initializer
            {
                let _ = scan.visit_expr(initializer);
            }
        }
        for inheritance in base.bases_args {
            let _ = scan.visit_modifier(inheritance);
        }
    }
    let mut trusted = Vec::new();
    for &function in reached.keys() {
        if is_trusted(gcx, function) {
            if !is_core_file(&gcx.hir.source(gcx.hir.function(function).source).file.name) {
                trusted.push(function);
            }
            continue;
        }
        if gcx.hir.function(function).is_yul {
            continue;
        }
        scan.current = Some(function);
        let _ = scan.visit_nested_function(function);
    }
    Findings { violations: scan.violations, trusted, reached }
}

/// Scans the bodies of the functions a contract runs for what a safe profile rejects.
struct Scan<'gcx> {
    gcx: Gcx<'gcx>,
    /// The function whose body is being scanned, or `None` for the creation code outside any
    /// function.
    current: Option<hir::FunctionId>,
    violations: Vec<Finding>,
    seen: FxHashSet<Span>,
}

impl Scan<'_> {
    fn record(&mut self, violation: Violation, span: Span) {
        if self.seen.insert(span) {
            self.violations.push(Finding { violation, span, function: self.current });
        }
    }
}

impl<'gcx> Visit<'gcx> for Scan<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        match stmt.kind {
            // Assembly is not Solidity, so nothing in it is scanned further.
            StmtKind::AssemblyBlock(block) => {
                if !is_namespace_accessor(self.gcx, &block) {
                    self.record(Violation::Assembly, stmt.span);
                }
                return ControlFlow::Continue(());
            }
            StmtKind::UncheckedBlock(_) => self.record(Violation::Unchecked, stmt.span),
            _ => {}
        }
        self.walk_stmt(stmt)
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        // A call to `Math.wrappingAdd`, `wrappingSub` or `wrappingMul` is reported as the call.
        if let Some((callee, _, _)) = expr.as_call()
            && let Some(function) = self.gcx.resolved_function(callee)
            && is_wrapping(self.gcx, function)
        {
            self.record(Violation::Wrapping, expr.span);
            self.seen.insert(callee.span);
        }
        // Any other reference takes the operation as a value, which a function pointer may call:
        // trusted, its body is never scanned.
        if let Some(function) = self.gcx.resolved_function(expr)
            && is_wrapping(self.gcx, function)
        {
            self.record(Violation::Wrapping, expr.span);
        }
        self.walk_expr(expr)
    }
}

/// Whether the function `id` is one of the wrapping operations of `solar:core/v1/Math.sol`.
fn is_wrapping(gcx: Gcx<'_>, id: hir::FunctionId) -> bool {
    let function = gcx.hir.function(id);
    matches!(
        &gcx.hir.source(function.source).file.name,
        FileName::Custom(path) if path == "solar:core/v1/Math.sol"
    ) && gcx.item_name(id).as_str().starts_with("wrapping")
}
