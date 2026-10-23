//! `@custom:solar-safe`: the safe profile of a contract or a library.
//!
//! The tag turns properties of all the code a contract runs in its own call frame into
//! requirements. `memory` requires that none of it is inline assembly, the only way Solidity code
//! reaches memory outside the objects it allocates. `arithmetic` requires that all of its
//! arithmetic is checked: no `unchecked` block, and no inline assembly either, whose arithmetic
//! wraps. A tag that names neither requires both. Other compilers read the tag as documentation.
//!
//! The code a contract runs is what its creation and its entry points reach through internal
//! calls, as the call graph resolves them for the contract: across its bases, libraries and free
//! functions, through modifiers and base constructors, from the initializers of its bases' state
//! variables and the arguments of their inheritance specifiers, to the override a virtual call
//! dispatches to, and to every function whose value is taken, which a function pointer may call.
//! External calls, calls to deployed libraries and contract creations run in call frames of their
//! own, with memory of their own, so they are not part of it. A library's code is all of its
//! functions.
//!
//! `@custom:solar-trusted` marks a function, a modifier, a contract or a library as reviewed: the
//! profile does not look inside it, so what only it reaches is not checked either, except where
//! its calls dispatch to an override or a `super` target other than the function they name: the
//! contract chose that code, not the review. It is a boundary that review, not this compiler,
//! answers for. Every `@custom:solar-safe` tag on a contract adds its properties.

use crate::{
    hir::{self, StmtKind, Visit},
    ty::{Gcx, traced_functions},
};
use solar_data_structures::{
    Never,
    map::{FxHashSet, FxIndexMap},
};
use solar_interface::{Span, Symbol, sym};
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

/// The `@custom:solar-safe` tags on the contract `id`, if it has any: their profile requires
/// every property one of them requires.
fn profile(gcx: Gcx<'_>, id: hir::ContractId) -> Option<Profile> {
    let contract = gcx.hir.contract(id);
    // The tag on an interface is reported with the documentation.
    if contract.kind == hir::ContractKind::Interface {
        return None;
    }
    let mut tags = gcx
        .hir
        .doc(contract.doc)
        .ast_comments
        .iter()
        .flat_map(|comment| comment.natspec.iter())
        .filter(|natspec| is_tag(natspec, sym::solar_dash_safe))
        .peekable();
    let tag = tags.peek()?.span;
    let (mut memory, mut arithmetic) = (false, false);
    for natspec in tags {
        let (mut tag_memory, mut tag_arithmetic) = (false, false);
        for property in natspec.content().split_whitespace() {
            match property {
                "memory" => tag_memory = true,
                "arithmetic" => tag_arithmetic = true,
                _ => {
                    gcx.dcx()
                        .err(format!("`@custom:solar-safe` has no property `{property}`"))
                        .span(natspec.span)
                        .help(
                            "the properties are `memory` and `arithmetic`; naming none requires \
                             both",
                        )
                        .emit();
                }
            }
        }
        if !tag_memory && !tag_arithmetic {
            (tag_memory, tag_arithmetic) = (true, true);
        }
        memory |= tag_memory;
        arithmetic |= tag_arithmetic;
    }
    Some(Profile { memory, arithmetic, tag })
}

/// Whether `natspec` is the custom tag `name`.
fn is_tag(natspec: &solar_ast::NatSpecItem, name: Symbol) -> bool {
    matches!(natspec.kind, solar_ast::NatSpecKind::Custom { name: tag } if tag.name == name)
}

/// Whether the function `id` is reviewed: tagged `@custom:solar-trusted` itself or in a contract
/// tagged so.
fn is_trusted(gcx: Gcx<'_>, id: hir::FunctionId) -> bool {
    let function = gcx.hir.function(id);
    is_tagged_trusted(gcx, function.doc)
        || function
            .contract
            .is_some_and(|contract| is_tagged_trusted(gcx, gcx.hir.contract(contract).doc))
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
        let (applies, what, help) = match finding.violation {
            Violation::Assembly => (
                profile.memory || profile.arithmetic,
                "inline assembly",
                "write it without assembly, or review it and tag its function \
                 `@custom:solar-trusted`",
            ),
            Violation::Unchecked => (
                profile.arithmetic,
                "an `unchecked` block",
                "use checked arithmetic, or review it and tag its function \
                 `@custom:solar-trusted`",
            ),
        };
        if !applies {
            continue;
        }
        gcx.dcx()
            .err(format!("`{name}` is tagged `@custom:solar-safe` but runs {what}"))
            .span(finding.span)
            .span_note(profile.tag, "the tag is here")
            .note(format!("it runs this through {}", findings.path(gcx, finding.function)))
            .help(help)
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
    /// The function whose body it is in.
    pub(crate) function: hir::FunctionId,
}

/// What a safe profile rejects.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Violation {
    /// Inline assembly, which breaks both properties.
    Assembly,
    /// An `unchecked` block.
    Unchecked,
}

impl Findings {
    /// The chain of functions through which the code reaches `function`, from its root.
    fn path(&self, gcx: Gcx<'_>, function: hir::FunctionId) -> String {
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
            current = self.reached.get(&id).copied().flatten();
        }
        names.reverse();
        names.join(" → ")
    }
}

/// Finds what the code the contract `id` runs does that a safe profile rejects.
pub(crate) fn findings(gcx: Gcx<'_>, id: hir::ContractId) -> Findings {
    let library = gcx.hir.contract(id).kind == hir::ContractKind::Library;
    let reached = traced_functions(gcx, id, library, &|function| is_trusted(gcx, function));
    let mut scan = Scan { gcx, current: None, violations: Vec::new(), seen: FxHashSet::default() };
    let mut trusted = Vec::new();
    for &function in reached.keys() {
        if is_trusted(gcx, function) {
            trusted.push(function);
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
    /// The function whose body is being scanned.
    current: Option<hir::FunctionId>,
    violations: Vec<Finding>,
    seen: FxHashSet<Span>,
}

impl Scan<'_> {
    fn record(&mut self, violation: Violation, span: Span) {
        let function = self.current.expect("scanning a function body");
        if self.seen.insert(span) {
            self.violations.push(Finding { violation, span, function });
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
            StmtKind::AssemblyBlock(_) => {
                self.record(Violation::Assembly, stmt.span);
                return ControlFlow::Continue(());
            }
            StmtKind::UncheckedBlock(_) => self.record(Violation::Unchecked, stmt.span),
            _ => {}
        }
        self.walk_stmt(stmt)
    }
}
