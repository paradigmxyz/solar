//! Successful exits from the external call through compiler-owned operations.
//!
//! `Return.abiEncoded`, `Return.raw`, `Calls.forward` and `Calls.forwardDelegate` end the whole
//! external call successfully from however deep in its internal calls they run, so what they
//! return has to be what the entry point the call came in through declares. Every contract that
//! can be deployed, and every library, is traced from each place its code starts, through the
//! internal calls it makes as the call graph resolves them for it:
//!
//! - its creation may reach none of them: a successful return from creation deploys the returned
//!   bytes as the contract's code;
//! - raw bytes, which `Return.raw` and the forwards return, may only end a call to the fallback
//!   function, whose output no ABI describes;
//! - `Return.abiEncoded(value)` may end a call to the fallback function, or to a function that
//!   returns exactly one value of the same ABI type.
//!
//! A typed internal function pointer call reaches functions with matching parameter and return
//! types. User assembly or delegated code can retype pointers, retaining all targets whose
//! values the contract takes. The operations themselves can only be called directly: their bodies
//! are the compiler's, which no check follows, so a pointer to one would end the call unchecked.
//! External calls, including calls to a deployed library's functions, start calls of their own.
//! Inline assembly that returns is not followed: it declares no output this check could compare.

use crate::{
    core::{CoreIntrinsic, intrinsic_of, is_core_file},
    hir::{self, Visit},
    ty::{ExecutionGraph, Gcx, TraceRoot, Ty, TyAbiPrinter, TyAbiPrinterMode},
};
use solar_data_structures::{
    Never,
    map::{FxHashMap, FxHashSet},
    smallvec::SmallVec,
};
use solar_interface::Span;
use std::{collections::VecDeque, ops::ControlFlow};

/// A call to a compiler-owned operation that returns from the external call.
struct Site<'gcx> {
    span: Span,
    output: Output<'gcx>,
}

/// What an operation ends the external call with.
#[derive(Clone, Copy)]
enum Output<'gcx> {
    /// Raw bytes.
    Raw,
    /// One ABI-encoded value of this type.
    Encoded(Ty<'gcx>),
}

pub(super) fn check(gcx: Gcx<'_>) {
    // Most compilations import neither module; avoid scanning their function declarations.
    if !gcx.hir.source_ids().any(|id| {
        matches!(
            crate::core::module_path(&gcx.hir.source(id).file.name),
            Some("solar:core/Return.sol" | "solar:core/Calls.sol")
        )
    }) {
        return;
    }
    let exits = gcx
        .hir
        .function_ids()
        .filter(|&id| intrinsic_of(gcx, id).is_some_and(CoreIntrinsic::returns_from_call))
        .collect::<FxHashSet<_>>();
    if exits.is_empty() {
        return;
    }
    for source in gcx.hir.source_ids() {
        if !is_core_file(&gcx.hir.source(source).file.name) {
            let mut taken = TakenExits { gcx, exits: &exits, called: FxHashSet::default() };
            let _ = taken.visit_nested_source(source);
        }
    }
    let mut collect = Collect { gcx, exits: &exits, current: None, sites: FxHashMap::default() };
    for id in gcx.hir.function_ids() {
        let function = gcx.hir.function(id);
        if function.body.is_some()
            && !function.is_yul
            && !is_core_file(&gcx.hir.source(function.source).file.name)
        {
            collect.current = Some(id);
            let _ = collect.visit_nested_function(id);
        }
    }
    let sites = collect.sites;
    if sites.is_empty() {
        return;
    }
    let mut reported = FxHashSet::default();
    for id in gcx.hir.contract_ids() {
        let contract = gcx.hir.contract(id);
        if !contract.can_be_deployed()
            || !gcx.contract_reachable_functions(id).iter().any(|f| sites.contains_key(&f))
        {
            continue;
        }
        let graph = ExecutionGraph::new(gcx, id);
        let mut roots = vec![(TraceRoot::Creation, None)];
        for function in gcx.interface_functions(id).iter().map(|f| f.id).chain(contract.receive) {
            let entry = gcx.hir.function(function);
            let output =
                abi_tuple(gcx, entry.returns.iter().map(|&ret| gcx.type_of_item(ret.into())));
            roots.push((TraceRoot::Function(function), Some(output)));
        }
        // A fallback imposes no output constraint. At most two distinct entry signatures need
        // reach a function: one suffices for a raw exit, and an encoded exit cannot match two.
        // Each edge is consequently visited at most twice, even with many shared entry points.
        let mut witnesses = Vec::<Witness>::new();
        let mut pending = VecDeque::new();
        for (root, &(entry, _)) in roots.iter().enumerate() {
            let functions = match entry {
                TraceRoot::Creation => graph.calls.get(&None).cloned().unwrap_or_default(),
                TraceRoot::Function(function) => vec![function],
            };
            for function in functions {
                pending.push_back(Witness { function, root, parent: None });
            }
        }
        let mut reached = FxHashMap::<_, SmallVec<[usize; 2]>>::default();
        while let Some(witness) = pending.pop_front() {
            let seen = reached.entry(witness.function).or_default();
            if seen.len() == 2
                || seen
                    .iter()
                    .any(|&previous| roots[witnesses[previous].root].1 == roots[witness.root].1)
            {
                continue;
            }
            let index = witnesses.len();
            seen.push(index);
            for site in sites.get(&witness.function).into_iter().flatten() {
                if !reported.contains(&site.span)
                    && check_site(
                        gcx,
                        id,
                        roots[witness.root].0,
                        roots[witness.root].1.as_deref(),
                        &describe_path(gcx, &witnesses, &witness),
                        witness.function,
                        site,
                    )
                {
                    reported.insert(site.span);
                }
            }
            for &function in graph.calls.get(&Some(witness.function)).into_iter().flatten() {
                pending.push_back(Witness { function, root: witness.root, parent: Some(index) });
            }
            witnesses.push(witness);
        }
    }
}

/// Checks `site` in `function` against the call that `root` starts in the contract `id`, and
/// reports whether it was rejected.
fn check_site<'gcx>(
    gcx: Gcx<'gcx>,
    id: hir::ContractId,
    root: TraceRoot,
    declared: Option<&str>,
    path: &str,
    function: hir::FunctionId,
    site: &Site<'gcx>,
) -> bool {
    let TraceRoot::Function(entry) = root else {
        let name = gcx.hir.contract(id).name;
        gcx.dcx()
            .err(format!(
                "this ends the creation of `{name}` and deploys what it returns as its code"
            ))
            .span(site.span)
            .note(format!("the creation reaches this through {path}"))
            .help("revert instead, or end calls only from functions the deployed contract runs")
            .emit();
        return true;
    };
    let entry_fn = gcx.hir.function(entry);
    let declared = declared.expect("a function entry has an output signature");
    let described = match entry_fn.name {
        Some(name) => format!("`{name}`"),
        None => "the receive function".to_string(),
    };
    let (message, help) = match site.output {
        Output::Raw => (
            format!(
                "this ends a call to {described} with raw bytes, which only the fallback function \
                 returns"
            ),
            "end the call with an ABI-encoded value, or forward from the fallback function",
        ),
        Output::Encoded(ty) => {
            let got = abi_tuple(gcx, [ty]);
            if got == declared {
                return false;
            }
            (
                format!(
                    "this ends a call to {described} with `{got}`, but it returns `{declared}`"
                ),
                "end the call with a value of the type the function returns",
            )
        }
    };
    let mut diag = gcx.dcx().err(message).span(site.span);
    if function != entry {
        diag = diag.note(format!("the call reaches this through {path}"));
    }
    let declaration = entry_fn.name.map_or_else(|| entry_fn.keyword_span(), |name| name.span);
    diag.span_note(declaration, format!("{described} is declared here")).help(help).emit();
    true
}

/// The ABI tuple of `tys`, as `(uint256,string)`.
fn abi_tuple<'gcx>(gcx: Gcx<'gcx>, tys: impl IntoIterator<Item = Ty<'gcx>>) -> String {
    let mut s = String::from("(");
    for (index, ty) in tys.into_iter().enumerate() {
        if index != 0 {
            s.push(',');
        }
        // Library storage references are ABI words, including recursive structs. Expanding
        // their fields as an ordinary ABI tuple is both incorrect and potentially recursive.
        if matches!(ty.kind, crate::ty::TyKind::Ref(_, solar_ast::DataLocation::Storage)) {
            s.push_str("uint256");
        } else {
            TyAbiPrinter::new(gcx, &mut s, TyAbiPrinterMode::Signature).print(ty).unwrap();
        }
    }
    s.push(')');
    s
}

/// Collects the calls to the operations in `exits` from every function body.
struct Collect<'gcx, 'a> {
    gcx: Gcx<'gcx>,
    exits: &'a FxHashSet<hir::FunctionId>,
    /// The function whose body is being visited.
    current: Option<hir::FunctionId>,
    sites: FxHashMap<hir::FunctionId, Vec<Site<'gcx>>>,
}

impl<'gcx> Visit<'gcx> for Collect<'gcx, '_> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let Some((callee, _, _)) = expr.as_call()
            && let Some(operation) = self.gcx.resolved_function(callee)
            && self.exits.contains(&operation)
            && let Some(current) = self.current
        {
            let output = match intrinsic_of(self.gcx, operation) {
                Some(CoreIntrinsic::ReturnAbiEncoded) => {
                    let parameter = self.gcx.hir.function(operation).parameters[0];
                    Output::Encoded(self.gcx.type_of_item(parameter.into()))
                }
                _ => Output::Raw,
            };
            self.sites.entry(current).or_default().push(Site { span: expr.span, output });
        }
        self.walk_expr(expr)
    }
}

/// Rejects the operations in `exits` wherever they are taken as values rather than called.
struct TakenExits<'gcx, 'a> {
    gcx: Gcx<'gcx>,
    exits: &'a FxHashSet<hir::FunctionId>,
    /// The callees of the calls visited so far.
    called: FxHashSet<hir::ExprId>,
}

impl<'gcx> Visit<'gcx> for TakenExits<'gcx, '_> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let Some((callee, _, _)) = expr.as_call() {
            self.called.insert(callee.peel_parens().id);
        } else if !self.called.contains(&expr.id)
            && let Some(operation) = self.gcx.resolved_function(expr)
            && self.exits.contains(&operation)
        {
            let function = self.gcx.hir.function(operation);
            let contract = function.contract.expect("call exits are library functions");
            let name = format!(
                "{}.{}",
                self.gcx.hir.contract(contract).name,
                function.name.expect("call exits are named")
            );
            self.gcx
                .dcx()
                .err(format!("`{name}` can only be called directly"))
                .span(expr.span)
                .note(
                    "it returns from the external call, which is checked against the entry \
                     point and any pending modifier code only where it is called",
                )
                .emit();
        }
        self.walk_expr(expr)
    }
}

/// One of at most two incompatible entry signatures reaching a function, with a shared path.
struct Witness {
    function: hir::FunctionId,
    root: usize,
    parent: Option<usize>,
}

fn describe_path(gcx: Gcx<'_>, witnesses: &[Witness], witness: &Witness) -> String {
    let mut names = Vec::new();
    let mut current = Some(witness);
    while let Some(witness) = current {
        if names.len() == 8 {
            names.push("…".to_string());
            break;
        }
        let f = gcx.hir.function(witness.function);
        let name = f.name_or_kind();
        names.push(match f.contract {
            Some(contract) => format!("`{}.{name}`", gcx.hir.contract(contract).name),
            None => format!("`{name}`"),
        });
        current = witness.parent.map(|index| &witnesses[index]);
    }
    names.reverse();
    names.join(" → ")
}
