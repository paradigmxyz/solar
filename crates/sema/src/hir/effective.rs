//! Runtime-aware HIR body traversal for lint and analysis passes.

use super::{Block, ContractId, Expr, Function, FunctionId, Hir, Modifier, Stmt, StmtKind, Visit};
use crate::ty::{CallInfo, Gcx};
use solar_data_structures::smallvec::SmallVec;
use std::{ops::ControlFlow, ptr};

/// Context supplied while visiting an effective function body.
///
/// The effective body includes applied modifiers and any internal calls which the visitor chooses
/// to follow. It retains the most-derived dispatch contract while inherited bodies are traversed.
#[derive(Clone, Copy)]
pub struct EffectiveBodyCx<'hir> {
    pub(super) gcx: Gcx<'hir>,
    pub(super) dispatch_contract: Option<ContractId>,
    pub(super) root_function: FunctionId,
    pub(super) function: Option<FunctionId>,
    pub(super) loop_depth: usize,
    pub(super) call_depth: usize,
    pub(super) call_entry_loop_depth: Option<usize>,
    pub(super) reports_enabled: bool,
}

impl<'hir> EffectiveBodyCx<'hir> {
    /// Returns the global compiler context.
    pub fn gcx(self) -> Gcx<'hir> {
        self.gcx
    }

    /// Returns the HIR map.
    pub fn hir(self) -> &'hir Hir<'hir> {
        &self.gcx.hir
    }

    /// Returns the most-derived contract used for internal virtual dispatch.
    pub fn dispatch_contract(self) -> Option<ContractId> {
        self.dispatch_contract
    }

    /// Returns the mutable state variables in this runtime dispatch context.
    pub fn mutable_state_variables(self) -> &'hir [super::VariableId] {
        let contract =
            self.dispatch_contract.or_else(|| self.hir().function(self.root_function).contract);
        contract.map_or(&[], |contract| self.gcx.contract_mutable_state_variables(contract))
    }

    /// Returns the root function passed to the traversal.
    pub fn root_function(self) -> FunctionId {
        self.root_function
    }

    /// Returns the function whose body is currently being traversed.
    pub fn function(self) -> FunctionId {
        self.function.unwrap_or(self.root_function)
    }

    /// Returns whether the current program point belongs to the root function body.
    pub fn is_root(self) -> bool {
        self.function.is_none()
    }

    /// Returns the internal call depth from the root function.
    pub fn call_depth(self) -> usize {
        self.call_depth
    }

    /// Returns the runtime loop nesting depth.
    pub fn loop_depth(self) -> usize {
        self.loop_depth
    }

    /// Returns whether this program point executes in a loop.
    pub fn in_loop(self) -> bool {
        self.loop_depth > 0
    }

    /// Returns whether this program point is in a root loop or a loop inherited from its caller.
    ///
    /// Local loops entered by an internal callee return `false`. This lets a lint follow a call
    /// made in a loop without reporting the callee's own loops again when each function is also
    /// checked independently.
    pub fn in_enclosing_loop(self) -> bool {
        self.in_loop() && self.call_entry_loop_depth.is_none_or(|depth| self.loop_depth <= depth)
    }

    /// Returns whether observations such as lint diagnostics should be emitted.
    ///
    /// Read-only visitors always return `true`. A flow analysis can disable reports while still
    /// applying transfer functions to an internal callee. Recursive summary solving also replays
    /// transfer callbacks with this set to `false`; the driver cannot suppress side effects in a
    /// consumer, so diagnostic emission must explicitly check this flag.
    pub fn reports_enabled(self) -> bool {
        self.reports_enabled
    }

    /// Returns semantic call information using this traversal's dispatch contract.
    pub fn call_info(self, expr: &'hir Expr<'hir>) -> Option<CallInfo<'hir>> {
        match self.dispatch_contract {
            Some(contract) => self.gcx.call_info_in_contract(expr, contract),
            None => self.gcx.call_info(expr),
        }
    }

    /// Returns variables owned by one internal-call activation.
    ///
    /// This includes the callee's parameters, returns, locals, and variables owned by its applied
    /// modifiers. Value/provenance domains use it to restore caller locals after normal return.
    pub fn activation_variables(self, callee: FunctionId) -> SmallVec<[super::VariableId; 16]> {
        let function = self.hir().function(callee);
        let mut owners = SmallVec::<[FunctionId; 4]>::from_slice(&[callee]);
        for modifier in function.modifiers {
            let Some(mut modifier) = modifier.id.as_function() else { continue };
            if let Some(contract) = self.dispatch_contract {
                modifier = self.gcx.modifier_in_contract(modifier, contract);
            }
            if !owners.contains(&modifier) {
                owners.push(modifier);
            }
        }
        owners
            .into_iter()
            .flat_map(|owner| self.hir().function_variables(owner).iter().copied())
            .collect()
    }
}

/// Callbacks for runtime-aware HIR body traversal.
///
/// This is intentionally shaped like a read-only compiler visitor: callbacks observe HIR nodes,
/// while the traversal owns modifier expansion, loop context, recursion guards, semantic call
/// resolution, and internal-call traversal.
pub trait EffectiveBodyVisitor<'hir> {
    /// The value returned when breaking from the traversal.
    type BreakValue;

    /// Visits a statement before its children.
    fn visit_stmt(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _stmt: &'hir Stmt<'hir>,
    ) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }

    /// Visits an expression before its children.
    fn visit_expr(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _expr: &'hir Expr<'hir>,
    ) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }

    /// Visits an expression after its children.
    fn visit_expr_post(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _expr: &'hir Expr<'hir>,
    ) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }

    /// Returns whether an internal call should be traversed.
    fn follow_internal_call(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _call: &'hir Expr<'hir>,
        _callee: FunctionId,
    ) -> bool {
        false
    }

    /// Visits the opaque branch of an unresolved internal function-value call.
    ///
    /// Known finite targets are offered through [`EffectiveBodyVisitor::follow_internal_call`]
    /// first. This callback then runs once when the value may additionally denote an unknown
    /// target, allowing conservative analyses without exposing function-value plumbing.
    fn visit_opaque_internal_call(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _call: &'hir Expr<'hir>,
    ) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }
}

/// Visits the runtime-effective body of `function`.
///
/// Applied modifiers are expanded at placeholders. Internal calls are resolved in the root
/// contract's dispatch context and followed when [`EffectiveBodyVisitor::follow_internal_call`]
/// returns `true`. Recursive call edges are cut.
pub fn visit_effective_body<'hir, V>(
    gcx: Gcx<'hir>,
    function: FunctionId,
    visitor: &mut V,
) -> ControlFlow<V::BreakValue>
where
    V: EffectiveBodyVisitor<'hir>,
{
    visit_effective_body_with_dispatch(gcx, function, gcx.hir.function(function).contract, visitor)
}

/// Visits the runtime-effective body of `function` as inherited by `dispatch_contract`.
pub fn visit_effective_body_in_contract<'hir, V>(
    gcx: Gcx<'hir>,
    function: FunctionId,
    dispatch_contract: ContractId,
    visitor: &mut V,
) -> ControlFlow<V::BreakValue>
where
    V: EffectiveBodyVisitor<'hir>,
{
    visit_effective_body_with_dispatch(gcx, function, Some(dispatch_contract), visitor)
}

/// Visits `function` in every contract dispatch context which can execute its body.
///
/// Contract functions are visited once for every contract where the declaration remains the
/// effective inherited implementation. Overriding entry points are analyzed through their own
/// bodies, including any explicit `super` calls. Free functions are visited once without a
/// contract context. A visitor which emits diagnostics should deduplicate source spans because the
/// same body can be observed in multiple contexts.
pub fn visit_effective_body_dispatches<'hir, V>(
    gcx: Gcx<'hir>,
    function: FunctionId,
    visitor: &mut V,
) -> ControlFlow<V::BreakValue>
where
    V: EffectiveBodyVisitor<'hir>,
{
    for dispatch_contract in effective_body_dispatch_contracts(gcx, function) {
        visit_effective_body_with_dispatch(gcx, function, dispatch_contract, visitor)?;
    }
    ControlFlow::Continue(())
}

pub(super) fn effective_body_dispatch_contracts(
    gcx: Gcx<'_>,
    function_id: FunctionId,
) -> SmallVec<[Option<ContractId>; 4]> {
    let function = gcx.hir.function(function_id);
    let Some(defining_contract) = function.contract else {
        let mut dispatches = SmallVec::new();
        dispatches.push(None);
        return dispatches;
    };
    if matches!(function.kind, super::FunctionKind::Constructor) {
        let mut dispatches = SmallVec::new();
        dispatches.push(Some(defining_contract));
        return dispatches;
    }
    gcx.hir
        .contract_ids()
        .filter(|&contract| {
            gcx.hir.contract(contract).linearized_bases.contains(&defining_contract)
                && gcx.function_in_contract(function_id, contract) == function_id
        })
        .map(Some)
        .collect()
}

fn visit_effective_body_with_dispatch<'hir, V>(
    gcx: Gcx<'hir>,
    root_function: FunctionId,
    dispatch_contract: Option<ContractId>,
    visitor: &mut V,
) -> ControlFlow<V::BreakValue>
where
    V: EffectiveBodyVisitor<'hir>,
{
    let function = gcx.hir.function(root_function);
    let Some(body) = function.body else { return ControlFlow::Continue(()) };
    EffectiveBodyWalker {
        gcx,
        visitor,
        root: function,
        root_function,
        dispatch_contract,
        current_function: None,
        loop_depth: 0,
        call_stack: Vec::new(),
        call_entry_loop_depths: Vec::new(),
        placeholder: None,
    }
    .visit_callable(function, body, None)
}

type ModifierContinuation<'hir> = (&'hir [Modifier<'hir>], usize, Block<'hir>, Option<FunctionId>);

struct EffectiveBodyWalker<'a, 'hir, V> {
    gcx: Gcx<'hir>,
    visitor: &'a mut V,
    root: &'hir Function<'hir>,
    root_function: FunctionId,
    dispatch_contract: Option<ContractId>,
    current_function: Option<FunctionId>,
    loop_depth: usize,
    call_stack: Vec<FunctionId>,
    call_entry_loop_depths: Vec<usize>,
    placeholder: Option<ModifierContinuation<'hir>>,
}

impl<'hir, V> EffectiveBodyWalker<'_, 'hir, V>
where
    V: EffectiveBodyVisitor<'hir>,
{
    fn cx(&self) -> EffectiveBodyCx<'hir> {
        EffectiveBodyCx {
            gcx: self.gcx,
            dispatch_contract: self.dispatch_contract,
            root_function: self.root_function,
            function: self.current_function,
            loop_depth: self.loop_depth,
            call_depth: self.call_stack.len(),
            call_entry_loop_depth: self.call_entry_loop_depths.last().copied(),
            reports_enabled: true,
        }
    }

    fn visit_callable(
        &mut self,
        function: &'hir Function<'hir>,
        body: Block<'hir>,
        function_id: Option<FunctionId>,
    ) -> ControlFlow<V::BreakValue> {
        self.visit_modifier_chain(function.modifiers, 0, body, function_id)
    }

    fn visit_modifier_chain(
        &mut self,
        modifiers: &'hir [Modifier<'hir>],
        index: usize,
        body: Block<'hir>,
        body_function: Option<FunctionId>,
    ) -> ControlFlow<V::BreakValue> {
        let Some(modifier) = modifiers.get(index) else {
            let previous_function = std::mem::replace(&mut self.current_function, body_function);
            let result = self.visit_block_with_placeholder(body, None);
            self.current_function = previous_function;
            return result;
        };

        self.visit_call_args(&modifier.args)?;
        let Some(mut modifier_id) = modifier.id.as_function() else {
            return self.visit_modifier_chain(modifiers, index + 1, body, body_function);
        };
        if let Some(dispatch_contract) = self.dispatch_contract {
            modifier_id = self.gcx.modifier_in_contract(modifier_id, dispatch_contract);
        }
        let modifier_function = self.gcx.hir.function(modifier_id);
        let Some(modifier_body) = modifier_function.body else {
            return self.visit_modifier_chain(modifiers, index + 1, body, body_function);
        };

        let previous_function = self.current_function.replace(modifier_id);
        let result = self.visit_block_with_placeholder(
            modifier_body,
            Some((modifiers, index + 1, body, body_function)),
        );
        self.current_function = previous_function;
        result
    }

    fn visit_block_with_placeholder(
        &mut self,
        block: Block<'hir>,
        placeholder: Option<ModifierContinuation<'hir>>,
    ) -> ControlFlow<V::BreakValue> {
        let previous = self.placeholder;
        self.placeholder = placeholder;
        let result = block.stmts.iter().try_for_each(|stmt| self.visit_stmt(stmt));
        self.placeholder = previous;
        result
    }

    fn visit_internal_call(&mut self, function_id: FunctionId) -> ControlFlow<V::BreakValue> {
        let function = self.gcx.hir.function(function_id);
        if ptr::eq(function, self.root) || self.call_stack.contains(&function_id) {
            return ControlFlow::Continue(());
        }
        let Some(body) = function.body else { return ControlFlow::Continue(()) };

        let previous_function = self.current_function.replace(function_id);
        self.call_stack.push(function_id);
        self.call_entry_loop_depths.push(self.loop_depth);
        let result = self.visit_callable(function, body, Some(function_id));
        self.call_entry_loop_depths.pop();
        self.call_stack.pop();
        self.current_function = previous_function;
        result
    }
}

impl<'hir, V> Visit<'hir> for EffectiveBodyWalker<'_, 'hir, V>
where
    V: EffectiveBodyVisitor<'hir>,
{
    type BreakValue = V::BreakValue;

    fn hir(&self) -> &'hir Hir<'hir> {
        &self.gcx.hir
    }

    fn visit_stmt(&mut self, stmt: &'hir Stmt<'hir>) -> ControlFlow<Self::BreakValue> {
        self.visitor.visit_stmt(self.cx(), stmt)?;
        match stmt.kind {
            StmtKind::Loop(block, _) => {
                self.loop_depth += 1;
                let result = block.stmts.iter().try_for_each(|stmt| self.visit_stmt(stmt));
                self.loop_depth -= 1;
                result
            }
            StmtKind::Placeholder => {
                if let Some((modifiers, index, body, body_function)) = self.placeholder {
                    self.visit_modifier_chain(modifiers, index, body, body_function)
                } else {
                    ControlFlow::Continue(())
                }
            }
            _ => self.walk_stmt(stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'hir Expr<'hir>) -> ControlFlow<Self::BreakValue> {
        self.visitor.visit_expr(self.cx(), expr)?;
        self.walk_expr(expr)?;

        let cx = self.cx();
        if let Some(info) = cx.call_info(expr)
            && info.function_ty().is_internal()
        {
            if let Some(function) = info.function() {
                if self.visitor.follow_internal_call(cx, expr, function) {
                    self.visit_internal_call(function)?;
                }
            } else if info.is_indirect_internal() {
                let targets = self.gcx.indirect_internal_call_targets(expr);
                let mut visited = SmallVec::<[FunctionId; 4]>::new();
                for &target in targets.known() {
                    let mut function = target.function();
                    if target.requires_virtual_dispatch()
                        && let Some(contract) = self.dispatch_contract
                    {
                        function = self.gcx.function_in_contract(function, contract);
                    }
                    if !visited.contains(&function) {
                        visited.push(function);
                        if self.visitor.follow_internal_call(cx, expr, function) {
                            self.visit_internal_call(function)?;
                        }
                    }
                }
                if targets.may_be_unknown() {
                    self.visitor.visit_opaque_internal_call(cx, expr)?;
                }
            }
        }
        self.visitor.visit_expr_post(self.cx(), expr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler;
    use solar_data_structures::Never;
    use solar_interface::{Session, config::CompileOpts};
    use std::path::PathBuf;

    const SOURCE: &str = r#"
contract Target {
    function ping() external {}
}

contract Factory {
    function getTarget() external returns (Target) {
        return new Target();
    }
}

contract C {
    modifier inLoop() {
        for (uint256 i; i < 1; ++i) {
            _;
        }
    }

    function helper(Target target) internal {
        target.ping();
        for (uint256 i; i < 1; ++i) {
            target.ping();
        }
    }

    function run(Factory factory) external inLoop {
        helper(factory.getTarget());
    }

    function opaque(function() internal callback) internal {
        callback();
    }
}
"#;

    #[derive(Debug, PartialEq, Eq)]
    struct CallEvent {
        phase: &'static str,
        callee: String,
        function: String,
        loop_depth: usize,
        call_depth: usize,
        in_enclosing_loop: bool,
    }

    #[derive(Default)]
    struct Recorder {
        calls: Vec<CallEvent>,
        loops: Vec<(String, usize)>,
        opaque_calls: Vec<String>,
    }

    impl Recorder {
        fn function_name(cx: EffectiveBodyCx<'_>) -> String {
            cx.gcx().item_canonical_name(cx.function()).to_string()
        }

        fn record_call<'hir>(
            &mut self,
            phase: &'static str,
            cx: EffectiveBodyCx<'hir>,
            expr: &'hir Expr<'hir>,
        ) {
            let Some(callee) = cx.call_info(expr).and_then(CallInfo::function) else { return };
            self.calls.push(CallEvent {
                phase,
                callee: cx.gcx().item_canonical_name(callee).to_string(),
                function: Self::function_name(cx),
                loop_depth: cx.loop_depth(),
                call_depth: cx.call_depth(),
                in_enclosing_loop: cx.in_enclosing_loop(),
            });
        }
    }

    impl<'hir> EffectiveBodyVisitor<'hir> for Recorder {
        type BreakValue = Never;

        fn visit_stmt(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            stmt: &'hir Stmt<'hir>,
        ) -> ControlFlow<Self::BreakValue> {
            if matches!(stmt.kind, StmtKind::Loop(..)) {
                self.loops.push((Self::function_name(cx), cx.loop_depth()));
            }
            ControlFlow::Continue(())
        }

        fn visit_expr(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            expr: &'hir Expr<'hir>,
        ) -> ControlFlow<Self::BreakValue> {
            self.record_call("pre", cx, expr);
            ControlFlow::Continue(())
        }

        fn visit_expr_post(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            expr: &'hir Expr<'hir>,
        ) -> ControlFlow<Self::BreakValue> {
            self.record_call("post", cx, expr);
            ControlFlow::Continue(())
        }

        fn follow_internal_call(
            &mut self,
            _cx: EffectiveBodyCx<'hir>,
            _call: &'hir Expr<'hir>,
            _callee: FunctionId,
        ) -> bool {
            true
        }

        fn visit_opaque_internal_call(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            _call: &'hir Expr<'hir>,
        ) -> ControlFlow<Self::BreakValue> {
            self.opaque_calls.push(Self::function_name(cx));
            ControlFlow::Continue(())
        }
    }

    #[test]
    fn traverses_effective_body_with_runtime_context() {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);

        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            let file =
                c.sess().source_map().new_source_file(PathBuf::from("test.sol"), SOURCE).unwrap();
            pcx.add_file(file);
            pcx.parse();

            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
        });

        compiler.enter(|c| {
            let gcx = c.gcx();
            let function_id = gcx
                .hir
                .function_ids()
                .find(|&id| gcx.item_canonical_name(id).to_string() == "C.run")
                .unwrap();
            let mut recorder = Recorder::default();
            assert_eq!(
                visit_effective_body(gcx, function_id, &mut recorder),
                ControlFlow::Continue(())
            );

            let calls = recorder
                .calls
                .iter()
                .map(|event| {
                    (
                        event.phase,
                        event.callee.as_str(),
                        event.function.as_str(),
                        event.loop_depth,
                        event.call_depth,
                        event.in_enclosing_loop,
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                calls,
                [
                    ("pre", "C.helper", "C.run", 1, 0, true),
                    ("pre", "Factory.getTarget", "C.run", 1, 0, true),
                    ("post", "Factory.getTarget", "C.run", 1, 0, true),
                    ("pre", "Target.ping", "C.helper", 1, 1, true),
                    ("post", "Target.ping", "C.helper", 1, 1, true),
                    ("pre", "Target.ping", "C.helper", 2, 1, false),
                    ("post", "Target.ping", "C.helper", 2, 1, false),
                    ("post", "C.helper", "C.run", 1, 0, true),
                ]
            );
            assert_eq!(recorder.loops, [("C.inLoop".to_string(), 0), ("C.helper".to_string(), 1)]);
            assert!(recorder.opaque_calls.is_empty());

            let opaque = gcx
                .hir
                .function_ids()
                .find(|&id| gcx.item_canonical_name(id).to_string() == "C.opaque")
                .unwrap();
            let mut recorder = Recorder::default();
            assert_eq!(visit_effective_body(gcx, opaque, &mut recorder), ControlFlow::Continue(()));
            assert_eq!(recorder.opaque_calls, ["C.opaque"]);
        });
    }
}
