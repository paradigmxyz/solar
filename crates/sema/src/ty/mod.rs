use crate::{
    Source, Sources, ast,
    ast_lowering::SymbolResolver,
    builtins::{Builtin, members},
    hir::{self, Hir, SourceId},
};
use alloy_primitives::{B256, Selector, U256, keccak256};
use either::Either;
use solar_ast::{DataLocation, StateMutability, TypeSize, UserDefinableOperator, Visibility};
use solar_data_structures::{
    BumpExt,
    bit_set::{DenseBitSet, GrowableBitSet},
    fmt::{from_fn, or_list},
    map::{FxBuildHasher, FxHashMap, FxHashSet},
    smallvec::SmallVec,
    trustme,
};
use solar_interface::{
    Ident, Session, Span, Symbol,
    config::CompilerStage,
    diagnostics::{DiagCtxt, ErrorGuaranteed},
    kw,
    source_map::{FileName, SourceFile},
    sym,
};
use std::{
    fmt,
    hash::Hash,
    ops::ControlFlow,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};
use thread_local::ThreadLocal;

mod print;
pub(crate) use print::TySolcPrinter;
pub use print::{TyAbiPrinter, TyAbiPrinterMode};

mod common;
pub use common::{CommonTypes, EachDataLoc};

mod interner;
use interner::Interner;

#[allow(clippy::module_inception)]
mod ty;
pub(crate) use ty::SameSourceFileLevelUserTypeError;
pub use ty::{Ty, TyConvertError, TyData, TyFlags, TyFn, TyFnKind, TyKind};

type FxOnceMap<K, V> = once_map::OnceMap<K, V, FxBuildHasher>;
type NatSpecContractKey = (Symbol, hir::SourceId);
type UsingDirectiveKey = usize;

/// A function exported by a contract.
#[derive(Clone, Copy, Debug)]
pub struct InterfaceFunction<'gcx> {
    /// The function ID.
    pub id: hir::FunctionId,
    /// The function 4-byte selector.
    pub selector: Selector,
    /// The function type.
    pub ty: Ty<'gcx>,
}

/// List of all the functions exported by a contract.
///
/// Return type of [`Gcx::interface_functions`].
#[derive(Clone, Copy, Debug)]
pub struct InterfaceFunctions<'gcx> {
    /// The exported functions along with their selector.
    pub functions: &'gcx [InterfaceFunction<'gcx>],
    /// The index in `functions` where the inherited functions start.
    pub inheritance_start: usize,
}

/// Sparse results produced by semantic type checking for later compiler stages.
#[derive(Clone, Debug, Default)]
pub struct TypeckResults<'gcx> {
    pub(crate) expr_types: FxHashMap<hir::ExprId, Ty<'gcx>>,
    pub(crate) resolved_callees: FxHashMap<hir::ExprId, ResolvedCallee>,
    pub(crate) resolved_members: FxHashMap<hir::ExprId, ResolvedMember>,
    pub(crate) unsupported_udvt_operators: GrowableBitSet<hir::ExprId>,
}

/// The target selected for a call callee expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResolvedCallee {
    pub res: hir::Res,
    /// Whether this member call was attached to the receiver through `using for`.
    pub attached: bool,
}

/// Semantic information about a type-checked function call.
#[derive(Clone, Copy, Debug)]
pub struct CallInfo<'gcx> {
    callee: &'gcx hir::Expr<'gcx>,
    function_ty: &'gcx TyFn<'gcx>,
    resolution: Option<ResolvedCallee>,
}

/// Execution-level classification of a type-checked call.
///
/// Unlike [`TyFnKind`], this normalizes equivalent Solidity and Yul operations so analyses do not
/// need to combine function-type and builtin predicates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CallKind {
    /// An ordinary internal call or builtin operation.
    Internal,
    /// A high-level external call.
    External,
    /// A low-level call, including `send` and `transfer`.
    Call,
    /// A static external call.
    StaticCall,
    /// A delegate or callcode external call.
    DelegateCall,
    /// High-level or Yul contract creation.
    Creation,
    /// A declaration value or an error-recovery callable with no execution classification.
    Other,
}

/// Gas made available to one external interaction.
///
/// This normalizes Solidity, legacy Yul, and EOF call forms so analyses do not need to recognize
/// `send`/`transfer`, explicit options, and `gasleft()` independently.
#[derive(Clone, Copy, Debug)]
pub enum CallGas<'gcx> {
    /// The fixed 2,300 gas stipend used by Solidity `send` and `transfer`.
    Stipend,
    /// A source expression explicitly requests the call's gas operand.
    Explicit {
        /// The requested gas operand.
        limit: &'gcx hir::Expr<'gcx>,
        /// Whether a possibly nonzero CALL/CALLCODE value adds the EVM's 2,300 gas stipend.
        value_stipend: bool,
    },
    /// The call forwards the execution-context gas, including explicit `gasleft()`/Yul `gas()`.
    Forwarded,
}

impl<'gcx> CallGas<'gcx> {
    /// The Solidity `send` and `transfer` gas stipend.
    pub const STIPEND: u64 = 2_300;

    /// Returns whether this is the fixed Solidity stipend.
    pub const fn is_stipend(self) -> bool {
        matches!(self, Self::Stipend)
    }

    /// Returns whether the call forwards execution-context gas.
    pub const fn is_forwarded(self) -> bool {
        matches!(self, Self::Forwarded)
    }

    /// Returns the explicit gas operand, if present.
    pub const fn explicit_limit(self) -> Option<&'gcx hir::Expr<'gcx>> {
        match self {
            Self::Explicit { limit, .. } => Some(limit),
            Self::Stipend | Self::Forwarded => None,
        }
    }

    /// Returns whether a possibly nonzero value operand adds the EVM call stipend.
    pub const fn adds_value_stipend(self) -> bool {
        matches!(self, Self::Explicit { value_stipend: true, .. })
    }

    /// Returns whether the call may receive more than `limit` gas.
    ///
    /// Non-constant explicit expressions remain conservative.
    pub fn may_exceed(self, gcx: Gcx<'gcx>, limit: u64) -> bool {
        match self {
            Self::Stipend => Self::STIPEND > limit,
            Self::Forwarded => true,
            Self::Explicit { limit: gas, value_stipend } => {
                let stipend = if value_stipend { Self::STIPEND } else { 0 };
                if stipend > limit {
                    return true;
                }
                gcx.try_eval_const_u256_wrapping(gas)
                    .is_none_or(|gas| gas > U256::from(limit - stipend))
            }
        }
    }
}

/// An unconditional EVM-level termination performed by a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallTermination {
    /// The current execution reverts.
    Revert,
    /// The current execution completes successfully without returning to its source continuation.
    SuccessfulHalt,
}

impl<'gcx> CallInfo<'gcx> {
    /// Returns the type-checked function type of the callee.
    pub fn function_ty(self) -> &'gcx TyFn<'gcx> {
        self.function_ty
    }

    /// Returns the selected callee resolution, if one was recorded.
    pub fn resolution(self) -> Option<ResolvedCallee> {
        self.resolution
    }

    /// Returns the selected function declaration, if known.
    pub fn function(self) -> Option<hir::FunctionId> {
        self.resolution
            .and_then(|resolved| resolved.res.as_function())
            .or(self.function_ty.function_id)
    }

    /// Returns whether this is a statically resolved internal function call.
    pub fn is_direct_internal(self) -> bool {
        self.kind() == CallKind::Internal && self.function().is_some() && self.builtin().is_none()
    }

    /// Returns whether this is an internal function-pointer call without a static target.
    pub fn is_indirect_internal(self) -> bool {
        self.kind() == CallKind::Internal
            && self.function().is_none()
            && self.builtin().is_none()
            && match self.resolution {
                Some(resolution) => resolution.res.as_variable().is_some(),
                None => !matches!(
                    self.callee.peel_parens().kind,
                    hir::ExprKind::New(_)
                        | hir::ExprKind::TypeCall(_)
                        | hir::ExprKind::Type(_)
                        | hir::ExprKind::Err(_)
                ),
            }
    }

    /// Returns the selected builtin, if this is a builtin call.
    pub fn builtin(self) -> Option<Builtin> {
        self.resolution.and_then(|resolved| resolved.res.as_builtin())
    }

    /// Returns whether the call uses member syntax with a receiver bound through `using for`.
    pub fn is_attached(self) -> bool {
        self.function_ty.attached || self.resolution.is_some_and(|resolved| resolved.attached)
    }

    /// Returns the normalized execution kind of this call.
    pub fn kind(self) -> CallKind {
        if let Some(builtin) = self.builtin() {
            match builtin {
                Builtin::AddressCall
                | Builtin::AddressPayableSend
                | Builtin::AddressPayableTransfer
                | Builtin::YulCall
                | Builtin::YulExtcall => return CallKind::Call,
                Builtin::AddressStaticcall | Builtin::YulStaticcall | Builtin::YulExtstaticcall => {
                    return CallKind::StaticCall;
                }
                Builtin::AddressDelegatecall
                | Builtin::YulCallcode
                | Builtin::YulDelegatecall
                | Builtin::YulExtdelegatecall => return CallKind::DelegateCall,
                Builtin::YulCreate | Builtin::YulCreate2 => return CallKind::Creation,
                _ => {}
            }
        }
        match self.function_ty.kind() {
            TyFnKind::Internal | TyFnKind::InternalWithSelector => CallKind::Internal,
            TyFnKind::External => CallKind::External,
            TyFnKind::DelegateCall | TyFnKind::BareDelegateCall => CallKind::DelegateCall,
            TyFnKind::BareCall => CallKind::Call,
            TyFnKind::BareStaticCall => CallKind::StaticCall,
            TyFnKind::Creation => CallKind::Creation,
            TyFnKind::Declaration => CallKind::Other,
        }
    }

    /// Returns whether this is high-level or Yul contract creation.
    pub fn is_contract_creation(self) -> bool {
        self.kind() == CallKind::Creation
    }

    /// Returns whether executing this call performs an external interaction.
    ///
    /// This includes high-level external calls, public library delegate calls, low-level address
    /// calls, contract creation, and the address `send` and `transfer` builtins.
    pub fn is_external_interaction(self) -> bool {
        matches!(
            self.kind(),
            CallKind::External
                | CallKind::Call
                | CallKind::StaticCall
                | CallKind::DelegateCall
                | CallKind::Creation
        )
    }

    /// Returns whether this external interaction may modify persistent state or emit logs.
    ///
    /// View and pure calls, including low-level `staticcall`, return `false`.
    pub fn is_state_mutating_external_interaction(self) -> bool {
        self.is_external_interaction() && self.may_mutate_state()
    }

    /// Returns whether the call may modify persistent state or emit logs.
    ///
    /// Unlike [`CallInfo::is_state_mutating_external_interaction`], this also returns `true` for
    /// mutating internal calls.
    pub fn may_mutate_state(self) -> bool {
        match self.builtin() {
            Some(builtin) if builtin.is_yul() => builtin.may_mutate_state(),
            _ => !matches!(
                self.function_ty.state_mutability,
                StateMutability::Pure | StateMutability::View
            ),
        }
    }

    /// Returns whether executing this call may change storage or make earlier storage reads stale.
    ///
    /// This excludes event emission, unlike [`CallInfo::may_mutate_state`]. Non-static external
    /// interactions remain conservative because reentrancy can change the current contract's
    /// storage before the call returns.
    pub fn may_write_state(self) -> bool {
        if self
            .resolution
            .is_some_and(|resolved| matches!(resolved.res, hir::Res::Item(hir::ItemId::Event(_))))
        {
            return false;
        }
        if let Some(builtin) = self.builtin() {
            return builtin.may_write_state()
                || (builtin.is_external_interaction() && self.kind() != CallKind::StaticCall);
        }
        (self.function().is_some() || self.is_indirect_internal() || self.is_external_interaction())
            && !matches!(
                self.function_ty.state_mutability,
                StateMutability::Pure | StateMutability::View
            )
    }

    /// Returns whether `call` may move native value out of the current contract.
    ///
    /// This normalizes source and Yul call forms, ignores statically-zero values and transfers to
    /// the current contract, and treats delegate execution as an exit capability because the
    /// target can destroy or otherwise transfer the caller's balance.
    pub fn may_transfer_native_value(self, gcx: Gcx<'gcx>, call: &'gcx hir::Expr<'gcx>) -> bool {
        let target_is_self =
            gcx.call_target(call).is_some_and(|target| gcx.expr_is_self_address(target));
        if matches!(self.builtin(), Some(Builtin::Selfdestruct | Builtin::YulSelfdestruct)) {
            return !target_is_self;
        }
        if self.kind() == CallKind::DelegateCall {
            return true;
        }
        let Some(value) = gcx.call_value(call) else { return false };
        if gcx.try_eval_const_value(value).is_ok_and(|value| value.is_zero()) {
            return false;
        }
        self.kind() == CallKind::Creation
            || matches!(self.kind(), CallKind::External | CallKind::Call) && !target_is_self
    }
}

impl ResolvedCallee {
    #[inline]
    pub fn new(res: hir::Res, attached: bool) -> Self {
        Self { res, attached }
    }
}

/// The target selected for a non-call member access expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResolvedMember {
    /// A member with a regular item or builtin resolution.
    Res(hir::Res),
    /// A struct field selected from the receiver type.
    StructField { struct_id: hir::StructId, field_index: usize },
    /// An enum variant selected from `Enum.Variant`.
    EnumVariant { enum_id: hir::EnumId, variant_index: usize },
}

/// A completion candidate for a member visible on a receiver type.
#[derive(Clone, Copy, Debug)]
pub struct MemberCompletion<'gcx> {
    /// The visible member candidate.
    pub member: members::Member<'gcx>,
    /// The source-level target for the member, if one can be identified.
    pub resolved: Option<ResolvedMember>,
}

/// Parameter names available for a callable's visible arguments.
pub type CallableParamNames = SmallVec<[Option<Symbol>; 8]>;

/// The source declaration used for a callable's parameter names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CallableParamSource {
    /// A function-like declaration.
    Function {
        /// The function ID.
        id: hir::FunctionId,
        /// Whether the leading receiver parameter is not visible at the call site.
        ///
        /// Attached member calls strip the receiver from the visible call parameters,
        /// but named arguments still come from the original function declaration.
        skips_receiver: bool,
    },
    /// A variable declared with a function type.
    FunctionType(hir::VariableId),
    /// A struct constructor.
    Struct(hir::StructId),
    /// An event invocation.
    Event(hir::EventId),
    /// An error invocation.
    Error(hir::ErrorId),
    /// A builtin with named parameters.
    Builtin(Builtin),
}

/// The visible signature of a callable expression.
#[derive(Clone, Copy, Debug)]
pub struct CallableSignature<'gcx> {
    /// The visible parameter types at the call site.
    pub parameters: &'gcx [Ty<'gcx>],
    /// The return types.
    pub returns: &'gcx [Ty<'gcx>],
    /// The declaration source for parameter names, if one exists.
    pub param_source: Option<CallableParamSource>,
}

impl<'gcx> TypeckResults<'gcx> {
    /// Returns the type inferred for the given expression, if available.
    #[inline]
    pub fn type_of_expr(&self, id: hir::ExprId) -> Option<Ty<'gcx>> {
        self.expr_types.get(&id).copied()
    }

    /// Returns the overload/member target selected for a call callee expression, if available.
    #[inline]
    pub fn resolved_callee(&self, id: hir::ExprId) -> Option<ResolvedCallee> {
        self.resolved_callees.get(&id).copied()
    }

    /// Returns the target selected for a non-call member access expression, if available.
    #[inline]
    pub fn resolved_member(&self, id: hir::ExprId) -> Option<ResolvedMember> {
        self.resolved_members.get(&id).copied()
    }

    /// Returns the selected builtin target for a non-call member access expression, if available.
    #[inline]
    pub fn builtin_member(&self, id: hir::ExprId) -> Option<Builtin> {
        match self.resolved_member(id)? {
            ResolvedMember::Res(hir::Res::Builtin(builtin)) => Some(builtin),
            _ => None,
        }
    }

    /// Returns the selected builtin target for a call callee expression, if available.
    #[inline]
    pub fn builtin_callee(&self, id: hir::ExprId) -> Option<Builtin> {
        match self.resolved_callee(id)?.res {
            hir::Res::Builtin(builtin) => Some(builtin),
            _ => None,
        }
    }

    /// Returns whether codegen cannot lower the user-defined operator used by this expression.
    #[inline]
    pub fn unsupported_udvt_operator(&self, id: hir::ExprId) -> bool {
        self.unsupported_udvt_operators.contains(id)
    }
}

impl<'gcx> InterfaceFunctions<'gcx> {
    /// Returns all the functions.
    pub fn all(&self) -> &'gcx [InterfaceFunction<'gcx>] {
        self.functions
    }

    /// Returns the defined functions.
    pub fn own(&self) -> &'gcx [InterfaceFunction<'gcx>] {
        &self.functions[..self.inheritance_start]
    }

    /// Returns the inherited functions.
    pub fn inherited(&self) -> &'gcx [InterfaceFunction<'gcx>] {
        &self.functions[self.inheritance_start..]
    }
}

impl<'gcx> std::ops::Deref for InterfaceFunctions<'gcx> {
    type Target = &'gcx [InterfaceFunction<'gcx>];

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.functions
    }
}

impl<'gcx> IntoIterator for InterfaceFunctions<'gcx> {
    type Item = &'gcx InterfaceFunction<'gcx>;
    type IntoIter = std::slice::Iter<'gcx, InterfaceFunction<'gcx>>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.functions.iter()
    }
}

/// Recursiveness of a type.
#[derive(Clone, Copy, Debug)]
pub enum Recursiveness {
    /// Not recursive.
    None,
    /// Recursive through indirection.
    Recursive,
    /// Recursive through direct reference. An error has already been emitted.
    Infinite(ErrorGuaranteed),
}

impl Recursiveness {
    /// Returns `true` if the type is not recursive.
    #[inline]
    pub fn is_none(self) -> bool {
        matches!(self, Self::None)
    }

    /// Returns `true` if the type is recursive.
    #[inline]
    pub fn is_recursive(self) -> bool {
        !self.is_none()
    }
}

/// Reference to the [global context](GlobalCtxt).
#[derive(Clone, Copy)]
#[cfg_attr(feature = "nightly", rustc_pass_by_value)]
pub struct Gcx<'gcx>(&'gcx GlobalCtxt<'gcx>);

impl<'gcx> std::ops::Deref for Gcx<'gcx> {
    type Target = &'gcx GlobalCtxt<'gcx>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'gcx> fmt::Debug for Gcx<'gcx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Transparent wrapper around `&'gcx mut GlobalCtxt<'gcx>`.
///
/// This uses a raw pointer because using `&mut` directly would make `'gcx` covariant and this just
/// is too annoying/impossible to deal with.
/// Since it's only used internally (`pub(crate)`), this is fine.
#[repr(transparent)]
pub(crate) struct GcxMut<'gcx>(*mut GlobalCtxt<'gcx>);

impl<'gcx> GcxMut<'gcx> {
    #[inline(always)]
    pub(crate) fn new(gcx: &mut GlobalCtxt<'gcx>) -> Self {
        Self(gcx)
    }

    #[inline(always)]
    pub(crate) fn get(&self) -> Gcx<'gcx> {
        unsafe { Gcx(&*self.0) }
    }

    #[inline(always)]
    pub(crate) fn get_mut(&mut self) -> &'gcx mut GlobalCtxt<'gcx> {
        unsafe { &mut *self.0 }
    }
}

impl<'gcx> std::ops::Deref for GcxMut<'gcx> {
    type Target = &'gcx mut GlobalCtxt<'gcx>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe { core::mem::transmute(self) }
    }
}

impl<'gcx> std::ops::DerefMut for GcxMut<'gcx> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::mem::transmute(self) }
    }
}

#[cfg(test)]
fn _gcx_traits() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Gcx<'static>>();
}

struct AtomicCompilerStage(AtomicUsize);

impl AtomicCompilerStage {
    fn new() -> Self {
        Self(AtomicUsize::new(usize::MAX))
    }

    fn set(&self, stage: CompilerStage) {
        self.0.store(stage as usize, Ordering::Relaxed);
    }

    fn get(&self) -> Option<CompilerStage> {
        let stage = self.0.load(Ordering::Relaxed);
        if stage == usize::MAX { None } else { Some(CompilerStage::from_repr(stage).unwrap()) }
    }
}

/// The global compilation context.
pub struct GlobalCtxt<'gcx> {
    pub sess: &'gcx Session,
    pub sources: Sources<'gcx>,
    pub(crate) symbol_resolver: SymbolResolver<'gcx>,
    pub hir: Hir<'gcx>,
    stage: AtomicCompilerStage,

    pub types: CommonTypes<'gcx>,
    typeck_results: OnceLock<TypeckResults<'gcx>>,

    pub(crate) ast_arenas: ThreadLocal<ast::Arena>,
    pub(crate) hir_arenas: ThreadLocal<hir::Arena>,
    interner: Interner<'gcx>,
    cache: Cache<'gcx>,
    pub(crate) inherited_override_functions:
        FxOnceMap<hir::ContractId, &'gcx crate::typeck::override_checker::InheritedFunctions<'gcx>>,
}

impl fmt::Debug for GlobalCtxt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlobalCtxt")
            .field("stage", &self.stage.get())
            .field("sess", self.sess)
            .field("sources", &self.sources.len())
            .finish_non_exhaustive()
    }
}

impl<'gcx> GlobalCtxt<'gcx> {
    pub(crate) fn new(sess: &'gcx Session) -> Self {
        let interner = Interner::new();
        let hir_arenas = ThreadLocal::<hir::Arena>::new();
        Self {
            sess,
            sources: Sources::new(),
            symbol_resolver: SymbolResolver::new(&sess.dcx),
            hir: Hir::new(),
            stage: AtomicCompilerStage::new(),

            // SAFETY: stable address because ThreadLocal holds the arenas through indirection.
            types: CommonTypes::new(
                &interner,
                unsafe { trustme::decouple_lt(&hir_arenas) }.get_or_default().bump(),
            ),
            typeck_results: Default::default(),

            ast_arenas: ThreadLocal::new(),
            hir_arenas,
            interner,
            cache: Cache::default(),
            inherited_override_functions: FxOnceMap::default(),
        }
    }
}

impl<'gcx> Gcx<'gcx> {
    pub(crate) fn new(gcx: &'gcx GlobalCtxt<'gcx>) -> Self {
        Self(gcx)
    }

    /// Returns the current compiler stage.
    pub fn stage(&self) -> Option<CompilerStage> {
        self.stage.get()
    }

    pub(crate) fn advance_stage(&self, to: CompilerStage) -> ControlFlow<()> {
        let from = self.stage();
        let result = self.advance_stage_(to);
        trace!(?from, ?to, ?result, "advance stage");
        result
    }

    fn advance_stage_(&self, to: CompilerStage) -> ControlFlow<()> {
        let current = self.stage();

        // Special case: allow calling `parse` multiple times while currently parsing.
        if to == CompilerStage::Parsing && current == Some(to) {
            return ControlFlow::Continue(());
        }

        let next = CompilerStage::next_opt(current);
        if next.is_none_or(|next| to != next) {
            let current_s = match current {
                Some(s) => s.to_str(),
                None => "none",
            };
            let next_s = match next {
                Some(s) => &format!("`{s}`"),
                None => "none (current stage is the last)",
            };
            self.dcx()
                .bug(format!(
                    "invalid compiler stage transition: cannot advance from `{current_s}` to `{to}`"
                ))
                .note(format!("expected next stage: {next_s}"))
                .note("stages must be advanced sequentially")
                .emit();
        }

        if let Some(current) = current
            && self.sess.stop_after(current)
        {
            return ControlFlow::Break(());
        }

        self.stage.set(to);
        ControlFlow::Continue(())
    }

    /// Returns the diagnostics context.
    pub fn dcx(self) -> &'gcx DiagCtxt {
        &self.sess.dcx
    }

    pub fn arena(self) -> &'gcx hir::Arena {
        self.hir_arenas.get_or_default()
    }

    pub fn bump(self) -> &'gcx bumpalo::Bump {
        self.arena().bump()
    }

    pub fn alloc<T>(self, value: T) -> &'gcx T {
        self.bump().alloc(value)
    }

    pub fn mk_ty(self, kind: TyKind<'gcx>) -> Ty<'gcx> {
        self.interner.intern_ty(self.bump(), kind)
    }

    pub fn mk_tys(self, tys: &[Ty<'gcx>]) -> &'gcx [Ty<'gcx>] {
        self.interner.intern_tys(self.bump(), tys)
    }

    pub fn mk_ty_iter(self, tys: impl Iterator<Item = Ty<'gcx>>) -> &'gcx [Ty<'gcx>] {
        self.interner.intern_ty_iter(self.bump(), tys)
    }

    pub fn mk_ty_tuple(self, tys: &'gcx [Ty<'gcx>]) -> Ty<'gcx> {
        self.mk_ty(TyKind::Tuple(tys))
    }

    pub(crate) fn mk_item_tys<T: Into<hir::ItemId> + Copy>(self, ids: &[T]) -> &'gcx [Ty<'gcx>] {
        self.mk_ty_iter(ids.iter().map(|&id| self.type_of_item(id.into())))
    }

    pub fn mk_ty_string_literal(self, s: &[u8]) -> Ty<'gcx> {
        self.mk_ty(TyKind::StringLiteral(
            std::str::from_utf8(s).is_ok(),
            TypeSize::new_int_bits(s.len().min(32) as u16 * 8),
        ))
    }

    pub fn mk_ty_int_literal(self, negative: bool, bits: u64) -> Option<Ty<'gcx>> {
        self.mk_ty_int_literal_with_fixed_bytes(negative, bits, None)
    }

    pub fn mk_ty_int_literal_with_fixed_bytes(
        self,
        negative: bool,
        bits: u64,
        compatible_fixed_bytes: Option<TypeSize>,
    ) -> Option<Ty<'gcx>> {
        let bits = bits.max(1);
        if bits > TypeSize::MAX as u64 {
            return None;
        }
        Some(self.mk_ty(TyKind::IntLiteral(
            negative,
            TypeSize::new_literal_bits(bits as u16),
            compatible_fixed_bytes,
        )))
    }

    pub fn mk_ty_fn(self, ptr: TyFn<'gcx>) -> Ty<'gcx> {
        self.mk_ty(TyKind::Fn(self.interner.intern_ty_fn(self.bump(), ptr)))
    }

    /// Returns the type inferred for the given expression, if available.
    ///
    /// Expression types are populated by the type checker.
    #[inline]
    pub fn type_of_expr(self, id: hir::ExprId) -> Option<Ty<'gcx>> {
        self.typeck_results.get()?.type_of_expr(id)
    }

    /// Returns the source argument at the given visible parameter index.
    ///
    /// Named arguments are reordered into the selected callable's declaration order. For an
    /// attached `using for` call, the implicit receiver is not part of this indexing. Positional
    /// variadic calls are indexed by their source arguments, and explicit type conversions expose
    /// their single argument at index zero.
    ///
    /// Returns `None` if type checking did not identify a callable target. This query maps source
    /// arguments but does not revalidate their types.
    pub fn call_arg(
        self,
        call: &hir::Expr<'gcx>,
        parameter_index: usize,
    ) -> Option<&'gcx hir::Expr<'gcx>> {
        let hir::ExprKind::Call(callee, args, _) = call.peel_parens().kind else { return None };
        let callee_ty = self.type_of_expr(callee.id)?;
        let signature = self.callable_signature_of_ty(callee_ty);

        let parameter_names = match args.kind {
            hir::CallArgsKind::Unnamed(_) => {
                let valid_index = signature.is_some_and(|signature| {
                    parameter_index < signature.parameters.len()
                        || signature
                            .parameters
                            .last()
                            .is_some_and(|ty| matches!(ty.kind, TyKind::Variadic))
                }) || matches!(callee_ty.kind, TyKind::Type(_))
                    && parameter_index == 0;
                if !valid_index {
                    return None;
                }
                None
            }
            hir::CallArgsKind::Named(_) => {
                let signature = signature?;
                if parameter_index >= signature.parameters.len() {
                    return None;
                }
                let source = self.call_param_source(callee).or(signature.param_source)?;
                Some(self.callable_param_names(source))
            }
        };
        args.argument_for_parameter(parameter_index, parameter_names.as_deref())
    }

    /// Returns the source argument bound to a declared function parameter.
    ///
    /// Unlike [`Gcx::call_arg`], parameter index zero returns the implicit receiver of an attached
    /// `using for` call. Ordinary, static library, and named-argument calls retain declaration
    /// order.
    pub fn call_arg_for_param(
        self,
        call: &hir::Expr<'gcx>,
        parameter_index: usize,
    ) -> Option<&'gcx hir::Expr<'gcx>> {
        let info = self.call_info(call)?;
        if info.is_attached() {
            if parameter_index == 0 {
                return self.call_receiver(call);
            }
            self.call_arg(call, parameter_index - 1)
        } else {
            self.call_arg(call, parameter_index)
        }
    }

    /// Returns the receiver of a member call.
    pub fn call_receiver(self, call: &hir::Expr<'gcx>) -> Option<&'gcx hir::Expr<'gcx>> {
        let hir::ExprKind::Call(callee, ..) = call.peel_parens().kind else { return None };
        let hir::ExprKind::Member(receiver, _) = callee.peel_parens().kind else { return None };
        Some(receiver)
    }

    /// Returns the explicit account target of a call-like expression.
    ///
    /// Source member calls use their receiver. Yul call and selfdestruct builtins use their
    /// normalized positional target. Contract creation has no pre-existing target.
    pub fn call_target(self, call: &hir::Expr<'gcx>) -> Option<&'gcx hir::Expr<'gcx>> {
        let info = self.call_info(call)?;
        if info.is_attached() && info.kind() == CallKind::DelegateCall {
            return None;
        }
        if let Some(receiver) = self.call_receiver(call) {
            return Some(receiver);
        }
        match info.builtin()? {
            Builtin::YulCall
            | Builtin::YulCallcode
            | Builtin::YulDelegatecall
            | Builtin::YulStaticcall => self.call_arg(call, 1),
            Builtin::YulExtcall
            | Builtin::YulExtdelegatecall
            | Builtin::YulExtstaticcall
            | Builtin::Selfdestruct
            | Builtin::YulSelfdestruct => self.call_arg(call, 0),
            _ => None,
        }
    }

    /// Returns a named call-option value such as `gas` or `value`.
    pub fn call_option(
        self,
        call: &hir::Expr<'gcx>,
        name: Symbol,
    ) -> Option<&'gcx hir::Expr<'gcx>> {
        let hir::ExprKind::Call(_, _, Some(options)) = call.peel_parens().kind else { return None };
        options.args.iter().find(|option| option.name.name == name).map(|option| &option.value)
    }

    /// Returns the explicit gas limit supplied to a call.
    ///
    /// This normalizes Solidity `{gas: ...}` options and the leading gas argument of legacy Yul
    /// call opcodes. EOF `ext*call` builtins do not expose a gas operand.
    pub fn call_gas_limit(self, call: &hir::Expr<'gcx>) -> Option<&'gcx hir::Expr<'gcx>> {
        if let Some(gas) = self.call_option(call, kw::Gas) {
            return Some(gas);
        }
        match self.call_info(call)?.builtin()? {
            Builtin::YulCall
            | Builtin::YulCallcode
            | Builtin::YulDelegatecall
            | Builtin::YulStaticcall => self.call_arg(call, 0),
            _ => None,
        }
    }

    /// Returns the normalized gas behavior of an external interaction.
    pub fn call_gas(self, call: &'gcx hir::Expr<'gcx>) -> Option<CallGas<'gcx>> {
        let info = self.call_info(call)?;
        if !info.is_external_interaction() {
            return None;
        }
        if matches!(
            info.builtin(),
            Some(Builtin::AddressPayableSend | Builtin::AddressPayableTransfer)
        ) {
            return Some(CallGas::Stipend);
        }
        let Some(gas) = self.call_gas_limit(call) else { return Some(CallGas::Forwarded) };
        if matches!(
            self.call_info(gas).and_then(|info| info.builtin()),
            Some(Builtin::Gasleft | Builtin::YulGas)
        ) {
            Some(CallGas::Forwarded)
        } else {
            let value_stipend = (matches!(info.kind(), CallKind::External | CallKind::Call)
                || info.builtin() == Some(Builtin::YulCallcode))
                && self.call_value(call).is_some_and(|value| {
                    !self.try_eval_const_value(value).is_ok_and(|v| v.is_zero())
                });
            Some(CallGas::Explicit { limit: gas, value_stipend })
        }
    }

    /// Returns the explicit native value supplied to a call or creation.
    ///
    /// This normalizes Solidity `{value: ...}` options, `send` and `transfer`, and the positional
    /// value arguments of Yul call and creation builtins.
    pub fn call_value(self, call: &hir::Expr<'gcx>) -> Option<&'gcx hir::Expr<'gcx>> {
        if let Some(value) = self.call_option(call, sym::value) {
            return Some(value);
        }
        match self.call_info(call)?.builtin()? {
            Builtin::AddressPayableSend | Builtin::AddressPayableTransfer => self.call_arg(call, 0),
            Builtin::YulCreate | Builtin::YulCreate2 => self.call_arg(call, 0),
            Builtin::YulCall | Builtin::YulCallcode => self.call_arg(call, 2),
            Builtin::YulExtcall => self.call_arg(call, 3),
            _ => None,
        }
    }

    /// Returns the native value transferred to another account by a call or creation.
    ///
    /// Unlike [`Gcx::call_value`], this excludes CALLCODE's value operand, which defines the
    /// callee's call-context value without transferring balance to the target.
    pub fn call_transferred_value(self, call: &hir::Expr<'gcx>) -> Option<&'gcx hir::Expr<'gcx>> {
        matches!(
            self.call_info(call)?.kind(),
            CallKind::External | CallKind::Call | CallKind::Creation
        )
        .then(|| self.call_value(call))
        .flatten()
    }

    /// Returns the source argument bound to a modifier parameter.
    pub fn modifier_arg(
        self,
        modifier: &hir::Modifier<'gcx>,
        parameter_index: usize,
    ) -> Option<&'gcx hir::Expr<'gcx>> {
        let function = self.hir.function(modifier.id.as_function()?);
        let parameter = *function.parameters.get(parameter_index)?;
        match modifier.args.kind {
            hir::CallArgsKind::Unnamed(arguments) => arguments.get(parameter_index),
            hir::CallArgsKind::Named(arguments) => {
                let name = self.hir.variable(parameter).name?;
                arguments
                    .iter()
                    .find(|argument| argument.name == name)
                    .map(|argument| &argument.value)
            }
        }
    }

    /// Returns the parameter-name source for a type-checked call callee.
    ///
    /// The selected callable type takes precedence. Syntax and builtin semantics only recover
    /// parameter names when the selected signature does not carry a declaration source.
    pub fn call_param_source(self, callee: &hir::Expr<'gcx>) -> Option<CallableParamSource> {
        let callee = callee.peel_parens();
        if let Some(source) = self
            .type_of_expr(callee.id)
            .and_then(|ty| self.callable_signature_of_ty(ty))
            .and_then(|signature| signature.param_source)
        {
            return Some(source);
        }

        if let hir::ExprKind::Ident([res]) = callee.kind
            && let Some(id) = res.as_variable()
            && matches!(self.hir.variable(id).ty.kind, hir::TypeKind::Function(_))
        {
            return Some(CallableParamSource::FunctionType(id));
        }
        if let hir::ExprKind::Member(receiver, name) = callee.kind
            && let Some(receiver_ty) = self.type_of_expr(receiver.id)
            && let Some(ResolvedMember::StructField { struct_id, field_index }) =
                self.resolve_member_target(receiver_ty, name.name, None)
            && let Some(&id) = self.hir.strukt(struct_id).fields.get(field_index)
            && matches!(self.hir.variable(id).ty.kind, hir::TypeKind::Function(_))
        {
            return Some(CallableParamSource::FunctionType(id));
        }
        if let hir::ExprKind::New(hir_ty) = &callee.kind
            && let TyKind::Contract(id) = self.type_of_hir_ty(hir_ty).kind
            && let Some(id) = self.hir.contract(id).ctor
        {
            return Some(CallableParamSource::Function { id, skips_receiver: false });
        }
        if let Some(builtin) = self.builtin_callee(callee.id)
            && matches!(builtin, Builtin::AbiDecode)
        {
            return Some(CallableParamSource::Builtin(builtin));
        }
        None
    }

    /// Returns the overload/member target selected for a call callee expression, if available.
    #[inline]
    pub fn resolved_callee(self, id: hir::ExprId) -> Option<ResolvedCallee> {
        self.typeck_results.get()?.resolved_callee(id)
    }

    /// Returns the target selected for a full call expression.
    ///
    /// This preserves whether a member call is attached through `using for` and may resolve to a
    /// function-typed variable rather than a function declaration.
    #[inline]
    pub fn resolved_call(self, expr: &hir::Expr<'gcx>) -> Option<ResolvedCallee> {
        let hir::ExprKind::Call(callee, ..) = expr.peel_parens().kind else { return None };
        self.resolved_callee(callee.id)
    }

    /// Returns semantic information about a full type-checked function call expression.
    #[inline]
    pub fn call_info(self, expr: &hir::Expr<'gcx>) -> Option<CallInfo<'gcx>> {
        let hir::ExprKind::Call(callee, ..) = expr.peel_parens().kind else { return None };
        let TyKind::Fn(function_ty) = self.type_of_expr(callee.id)?.kind else { return None };
        Some(CallInfo { callee, function_ty, resolution: self.resolved_callee(callee.id) })
    }

    /// Returns an unconditional EVM-level termination performed by this call.
    ///
    /// Conditional failures such as `require(value)` return `None` unless the condition is a
    /// compile-time `false` value.
    pub fn call_termination(self, expr: &hir::Expr<'gcx>) -> Option<CallTermination> {
        let info = self.call_info(expr)?;
        match info.builtin()? {
            builtin if builtin.is_reverting_halt() => Some(CallTermination::Revert),
            builtin if builtin.is_successful_halt() => Some(CallTermination::SuccessfulHalt),
            Builtin::Require | Builtin::Assert => (self
                .call_arg(expr, 0)
                .and_then(|arg| self.try_eval_const_value(arg).ok())
                .and_then(|value| value.as_bool())
                == Some(false))
            .then_some(CallTermination::Revert),
            _ => None,
        }
    }

    /// Returns whether the call unconditionally reverts the current execution path.
    pub fn call_is_statically_aborting(self, expr: &hir::Expr<'gcx>) -> bool {
        self.call_termination(expr) == Some(CallTermination::Revert)
    }

    /// Returns semantic call information resolved for execution in `dispatch_contract`.
    ///
    /// Unlike [`Gcx::call_info`], this accounts for context-sensitive internal virtual dispatch
    /// and `super` calls in inherited function bodies. This is useful for analyses which traverse
    /// a base function as part of a more-derived contract.
    pub fn call_info_in_contract(
        self,
        expr: &hir::Expr<'gcx>,
        dispatch_contract: hir::ContractId,
    ) -> Option<CallInfo<'gcx>> {
        let mut info = self.call_info(expr)?;
        if !info.function_ty().is_internal() {
            return Some(info);
        }

        let hir::ExprKind::Call(callee, ..) = expr.peel_parens().kind else { unreachable!() };
        let Some(function) = info.function() else { return Some(info) };
        let Some(dispatched) = self.dispatched_function(callee, function, dispatch_contract) else {
            return Some(info);
        };

        let attached = info.resolution.is_some_and(|resolution| resolution.attached);
        info.resolution = Some(ResolvedCallee::new(hir::Res::Item(dispatched.into()), attached));
        let TyKind::Fn(function_ty) = self.type_of_item(dispatched.into()).kind else {
            unreachable!("function item must have a function type")
        };
        info.function_ty = function_ty;
        Some(info)
    }

    /// Resolves the implementation of a virtual function declaration in `dispatch_contract`.
    ///
    /// This models inherited entry-point dispatch rather than a particular call expression. It is
    /// useful when deciding which contracts can execute an inherited body. Constructors are never
    /// inherited, and non-virtual declarations resolve to themselves.
    pub fn function_in_contract(
        self,
        function_id: hir::FunctionId,
        dispatch_contract: hir::ContractId,
    ) -> hir::FunctionId {
        let function = self.hir.function(function_id);
        if matches!(function.kind, hir::FunctionKind::Constructor) {
            return function_id;
        }
        if matches!(function.kind, hir::FunctionKind::Modifier) {
            return self.modifier_in_contract(function_id, dispatch_contract);
        }
        if !function.virtual_ {
            return function_id;
        }
        let Some(defining_contract) = function.contract else { return function_id };
        let bases = self.hir.contract(dispatch_contract).linearized_bases;
        if !bases.contains(&defining_contract) {
            return function_id;
        }
        let Some(name) = function.name.map(|name| name.name) else {
            return self
                .hir
                .contract(dispatch_contract)
                .linearized_bases
                .iter()
                .find_map(|&contract_id| match function.kind {
                    hir::FunctionKind::Fallback => self.hir.contract(contract_id).fallback,
                    hir::FunctionKind::Receive => self.hir.contract(contract_id).receive,
                    _ => None,
                })
                .unwrap_or(function_id);
        };
        let parameters = self.item_parameter_types(function_id);
        bases
            .iter()
            .find_map(|&contract_id| {
                self.hir.contract(contract_id).functions().find(|&candidate_id| {
                    let candidate = self.hir.function(candidate_id);
                    candidate.kind == function.kind
                        && candidate.body.is_some()
                        && candidate.name.is_some_and(|candidate| candidate.name == name)
                        && self.item_parameter_types(candidate_id) == parameters
                })
            })
            .unwrap_or(function_id)
    }

    /// Resolves a virtual modifier declaration for execution in `dispatch_contract`.
    pub fn modifier_in_contract(
        self,
        modifier: hir::FunctionId,
        dispatch_contract: hir::ContractId,
    ) -> hir::FunctionId {
        let function = self.hir.function(modifier);
        if !function.virtual_ || !matches!(function.kind, hir::FunctionKind::Modifier) {
            return modifier;
        }
        let Some(defining_contract) = function.contract else { return modifier };
        let Some(name) = function.name else { return modifier };
        let bases = self.hir.contract(dispatch_contract).linearized_bases;
        if !bases.contains(&defining_contract) {
            return modifier;
        }
        let parameters = self.item_parameter_types(modifier);
        bases
            .iter()
            .find_map(|&contract_id| {
                self.hir.contract(contract_id).functions().find(|&candidate_id| {
                    let candidate = self.hir.function(candidate_id);
                    matches!(candidate.kind, hir::FunctionKind::Modifier)
                        && candidate.body.is_some()
                        && candidate.name == Some(name)
                        && self.item_parameter_types(candidate_id) == parameters
                })
            })
            .unwrap_or(modifier)
    }

    fn dispatched_function(
        self,
        callee: &hir::Expr<'gcx>,
        function_id: hir::FunctionId,
        dispatch_contract: hir::ContractId,
    ) -> Option<hir::FunctionId> {
        let function = self.hir.function(function_id);
        let defining_contract = function.contract?;
        let name = function.name?.name;
        let bases = self.hir.contract(dispatch_contract).linearized_bases;
        if !bases.contains(&defining_contract) {
            return None;
        }

        let start = match &callee.peel_parens().kind {
            hir::ExprKind::Ident(_) if function.virtual_ => 0,
            hir::ExprKind::Member(receiver, _)
                if let Some(ty) = self.type_of_expr(receiver.peel_parens().id)
                    && let TyKind::Type(ty) = ty.kind
                    && let TyKind::Super(current_contract) = ty.kind =>
            {
                bases.iter().position(|&id| id == current_contract)? + 1
            }
            _ => return None,
        };
        let parameters = self.item_parameter_types(function_id);

        bases[start..].iter().find_map(|&contract_id| {
            self.hir.contract(contract_id).functions().find(|&candidate_id| {
                let candidate = self.hir.function(candidate_id);
                candidate.is_ordinary()
                    && candidate.visibility > Visibility::Private
                    && candidate.visibility != Visibility::External
                    && candidate.body.is_some()
                    && candidate.name.is_some_and(|candidate| candidate.name == name)
                    && self.item_parameter_types(candidate_id) == parameters
            })
        })
    }

    /// Returns the target selected for a non-call member access expression, if available.
    #[inline]
    pub fn resolved_member(self, id: hir::ExprId) -> Option<ResolvedMember> {
        self.typeck_results.get()?.resolved_member(id)
    }

    /// Returns the selected builtin target for a non-call member access expression, if available.
    #[inline]
    pub fn builtin_member(self, id: hir::ExprId) -> Option<Builtin> {
        self.typeck_results.get()?.builtin_member(id)
    }

    /// Returns the selected builtin target for a call callee expression, if available.
    #[inline]
    pub fn builtin_callee(self, id: hir::ExprId) -> Option<Builtin> {
        self.typeck_results.get()?.builtin_callee(id)
    }

    /// Returns whether codegen cannot lower the user-defined operator used by this expression.
    #[inline]
    pub fn unsupported_udvt_operator(self, id: hir::ExprId) -> bool {
        self.typeck_results.get().is_some_and(|results| results.unsupported_udvt_operator(id))
    }

    /// Returns whether sparse type-checker results are available for codegen queries.
    #[inline]
    pub fn has_typeck_results(self) -> bool {
        self.typeck_results.get().is_some()
    }

    pub(crate) fn set_typeck_results(self, results: TypeckResults<'gcx>) {
        if self.typeck_results.set(results).is_err() {
            self.dcx().bug("typeck results are already initialized").emit();
        }
    }

    pub fn mk_ty_variadic(self) -> Ty<'gcx> {
        self.mk_ty(TyKind::Variadic)
    }

    pub fn mk_ty_fn_with_kind(
        self,
        kind: TyFnKind,
        parameters: &[Ty<'gcx>],
        state_mutability: StateMutability,
        returns: &[Ty<'gcx>],
    ) -> Ty<'gcx> {
        self.mk_ty_fn(TyFn {
            kind,
            parameters: self.mk_tys(parameters),
            returns: self.mk_tys(returns),
            state_mutability: fn_state_mutability(kind, state_mutability),
            function_id: None,
            attached: false,
        })
    }

    pub(crate) fn mk_builtin_fn(
        self,
        parameters: &[Ty<'gcx>],
        state_mutability: StateMutability,
        returns: &[Ty<'gcx>],
    ) -> Ty<'gcx> {
        self.mk_ty_fn_with_kind(TyFnKind::Internal, parameters, state_mutability, returns)
    }

    pub(crate) fn mk_yul_builtin_fn(self, parameters: usize, returns: usize) -> Ty<'gcx> {
        let parameters = vec![self.types.uint(256); parameters];
        let returns = vec![self.types.uint(256); returns];
        self.mk_builtin_fn(&parameters, StateMutability::NonPayable, &returns)
    }

    pub(crate) fn mk_creation_fn(
        self,
        parameters: &[Ty<'gcx>],
        state_mutability: StateMutability,
        returns: &[Ty<'gcx>],
    ) -> Ty<'gcx> {
        self.mk_ty_fn_with_kind(TyFnKind::Creation, parameters, state_mutability, returns)
    }

    pub(crate) fn mk_builtin_mod(self, builtin: Builtin) -> Ty<'gcx> {
        self.mk_ty(TyKind::BuiltinModule(builtin))
    }

    pub fn mk_ty_misc_err(self) -> Ty<'gcx> {
        if let Err(e) = self.dcx().has_errors() {
            self.mk_ty_err(e)
        } else {
            self.dcx().bug("mk_ty_misc_err: no errors").emit()
        }
    }

    #[inline]
    pub fn mk_ty_err(self, guar: ErrorGuaranteed) -> Ty<'gcx> {
        const { assert!(std::mem::size_of::<ErrorGuaranteed>() == 0) }
        let _ = guar;
        self.types.__err_do_not_use
    }

    /// Returns the source file with the given path, if it exists.
    pub fn get_file(self, name: impl Into<FileName>) -> Option<Arc<SourceFile>> {
        self.sess.source_map().get_file(name)
    }

    /// Returns the AST source at the given path, if it exists.
    pub fn get_ast_source(
        self,
        name: impl Into<FileName>,
    ) -> Option<(SourceId, &'gcx Source<'gcx>)> {
        let file = self.get_file(name)?;
        self.sources.get_file(&file)
    }

    /// Returns the HIR source at the given path, if it exists.
    pub fn get_hir_source(
        self,
        name: impl Into<FileName>,
    ) -> Option<(SourceId, &'gcx hir::Source<'gcx>)> {
        let file = self.get_file(name)?;
        self.hir.sources.iter_enumerated().find(|(_, source)| Arc::ptr_eq(&source.file, &file))
    }

    /// Returns the name of the given item.
    ///
    /// # Panics
    ///
    /// Panics if the item has no name, such as unnamed function parameters.
    pub fn item_name(self, id: impl Into<hir::ItemId>) -> Ident {
        let id = id.into();
        self.item_name_opt(id).unwrap_or_else(|| panic!("item_name: missing name for item {id:?}"))
    }

    /// Returns the canonical name of the given item.
    ///
    /// This is the name of the item prefixed by the name of the contract it belongs to.
    pub fn item_canonical_name(self, id: impl Into<hir::ItemId>) -> impl fmt::Display {
        self.item_canonical_name_(id.into())
    }
    fn item_canonical_name_(self, id: hir::ItemId) -> impl fmt::Display {
        let name = self.item_name(id);
        let contract = self.hir.item(id).contract().map(|id| self.item_name(id));
        from_fn(move |f| {
            if let Some(contract) = contract {
                write!(f, "{contract}.")?;
            }
            write!(f, "{name}")
        })
    }

    /// Returns the fully qualified name of the contract.
    pub fn contract_fully_qualified_name(
        self,
        id: hir::ContractId,
    ) -> impl fmt::Display + use<'gcx> {
        from_fn(move |f| {
            let c = self.hir.contract(id);
            let source = self.hir.source(c.source);
            write!(f, "{}:{}", source.file.name.display(), c.name)
        })
    }

    /// Returns an iterator over the fields of the given item.
    ///
    /// Accepts structs, functions, errors, and events.
    pub fn item_fields(
        self,
        id: impl Into<hir::ItemId>,
    ) -> impl Iterator<Item = (Ty<'gcx>, hir::VariableId)> {
        self.item_fields_(id.into())
    }

    fn item_fields_(self, id: hir::ItemId) -> impl Iterator<Item = (Ty<'gcx>, hir::VariableId)> {
        let tys = if let hir::ItemId::Struct(id) = id {
            self.struct_field_types(id)
        } else {
            self.item_parameter_types(id)
        };
        let params = self.item_parameters(id);
        debug_assert_eq!(tys.len(), params.len());
        std::iter::zip(tys.iter().copied(), params.iter().copied())
    }

    /// Returns the parameter variable declarations of the given function-like item.
    ///
    /// Also accepts structs.
    ///
    /// # Panics
    ///
    /// Panics if the item is not a function-like item or a struct.
    pub fn item_parameters(self, id: impl Into<hir::ItemId>) -> &'gcx [hir::VariableId] {
        let id = id.into();
        self.item_parameters_opt(id)
            .unwrap_or_else(|| panic!("item_parameters: invalid item {id:?}"))
    }

    /// Returns the parameter variable declarations of the given function-like item.
    ///
    /// Also accepts structs.
    pub fn item_parameters_opt(
        self,
        id: impl Into<hir::ItemId>,
    ) -> Option<&'gcx [hir::VariableId]> {
        self.hir.item(id).parameters()
    }

    /// Returns the return variable declarations of the given function-like item.
    ///
    /// # Panics
    ///
    /// Panics if the item is not a function-like item.
    pub fn item_parameter_types(self, id: impl Into<hir::ItemId>) -> &'gcx [Ty<'gcx>] {
        let id = id.into();
        self.item_parameter_types_opt(id)
            .unwrap_or_else(|| panic!("item_parameter_types: invalid item {id:?}"))
    }

    /// Returns the return variable declarations of the given function-like item.
    ///
    /// # Panics
    ///
    /// Panics if the item is not a function-like item.
    pub fn item_parameter_types_opt(self, id: impl Into<hir::ItemId>) -> Option<&'gcx [Ty<'gcx>]> {
        self.type_of_item(id.into()).parameters()
    }

    /// Returns the name of the given item.
    #[inline]
    pub fn item_name_opt(self, id: impl Into<hir::ItemId>) -> Option<Ident> {
        self.hir.item(id).name()
    }

    /// Returns the span of the given item.
    #[inline]
    pub fn item_span(self, id: impl Into<hir::ItemId>) -> Span {
        self.hir.item(id).span()
    }

    /// Returns the 4-byte selector of the given item. Only accepts functions and errors.
    ///
    /// # Panics
    ///
    /// Panics if the item is not a function or error.
    pub fn function_selector(self, id: impl Into<hir::ItemId>) -> Selector {
        let id = id.into();
        assert!(
            matches!(id, hir::ItemId::Function(_) | hir::ItemId::Error(_)),
            "function_selector: invalid item {id:?}"
        );
        self.item_selector(id)[..4].try_into().unwrap()
    }

    /// Returns the 32-byte selector of the given event.
    pub fn event_selector(self, id: hir::EventId) -> B256 {
        self.item_selector(id.into())
    }

    /// Computes the [`Ty`] of the given [`hir::Type`]. Not cached.
    pub fn type_of_hir_ty(self, ty: &hir::Type<'_>) -> Ty<'gcx> {
        let kind = match ty.kind {
            hir::TypeKind::Elementary(ty) => TyKind::Elementary(ty),
            hir::TypeKind::Array(array) => {
                let elem = self.type_of_hir_ty(&array.element);
                match array.size {
                    Some(size) => match crate::eval::eval_array_len(self, size) {
                        Ok(size) => TyKind::Array(elem, size),
                        Err(guar) => TyKind::Array(self.mk_ty_err(guar), U256::from(1)),
                    },
                    None => TyKind::DynArray(elem),
                }
            }
            hir::TypeKind::Function(f) => {
                let kind = if f.visibility == Visibility::External {
                    TyFnKind::External
                } else {
                    TyFnKind::Internal
                };
                return self.mk_ty_fn(TyFn {
                    kind,
                    parameters: self.mk_item_tys(f.parameters),
                    returns: self.mk_item_tys(f.returns),
                    state_mutability: fn_state_mutability(kind, f.state_mutability),
                    function_id: None,
                    attached: false,
                });
            }
            hir::TypeKind::Mapping(mapping) => {
                let key = self.type_of_hir_ty(&mapping.key);
                let value = self.type_of_hir_ty(&mapping.value);
                TyKind::Mapping(key, value)
            }
            hir::TypeKind::Custom(item) => return self.type_of_item_simple(item, ty.span),
            hir::TypeKind::Err(guar) => return self.mk_ty_err(guar),
        };
        self.mk_ty(kind)
    }

    /// Returns the target type of the given [`hir::UsingDirective`].
    pub(crate) fn type_of_using_directive(
        self,
        using: &'gcx hir::UsingDirective<'gcx>,
    ) -> Option<Ty<'gcx>> {
        self.type_of_using_directive_cached(using as *const _ as UsingDirectiveKey)
    }

    fn type_of_item_simple(self, id: hir::ItemId, span: Span) -> Ty<'gcx> {
        match id {
            hir::ItemId::Contract(_)
            | hir::ItemId::Struct(_)
            | hir::ItemId::Enum(_)
            | hir::ItemId::Udvt(_) => self.type_of_item(id),
            _ => {
                let msg = "name has to refer to a valid user-defined type";
                self.mk_ty_err(self.dcx().emit_err(span, msg))
            }
        }
    }

    /// Returns the type of the given [`hir::Res`].
    pub fn type_of_res(self, res: hir::Res) -> Ty<'gcx> {
        match res {
            hir::Res::Item(id) => {
                let ty = self.type_of_item(id);
                if is_value_ns(id) { ty } else { self.mk_ty(TyKind::Type(ty)) }
            }
            hir::Res::Namespace(id) => self.mk_ty(TyKind::Module(id)),
            hir::Res::Builtin(builtin) => builtin.ty(self),
            hir::Res::Err(guar) => self.mk_ty_err(guar),
        }
    }

    /// Returns the visible callable signature for the given type.
    pub fn callable_signature_of_ty(self, ty: Ty<'gcx>) -> Option<CallableSignature<'gcx>> {
        match ty.kind {
            TyKind::Fn(function_ty) => Some(CallableSignature {
                parameters: function_ty.parameters,
                returns: function_ty.returns,
                param_source: self.callable_param_source_for_fn(function_ty),
            }),
            TyKind::Event(parameters, id) => Some(CallableSignature {
                parameters,
                returns: Default::default(),
                param_source: Some(CallableParamSource::Event(id)),
            }),
            TyKind::Error(parameters, id) => Some(CallableSignature {
                parameters,
                returns: Default::default(),
                param_source: Some(CallableParamSource::Error(id)),
            }),
            TyKind::Type(ty) => self.struct_constructor_signature(ty),
            TyKind::Err(_) => None,
            _ => None,
        }
    }

    /// Returns the visible callable signature for a member call candidate.
    pub fn callable_signature_of_member(
        self,
        receiver_ty: Ty<'gcx>,
        member: &members::Member<'gcx>,
    ) -> Option<CallableSignature<'gcx>> {
        let TyKind::Fn(function_ty) = member.ty.kind else { return None };
        let (parameters, skips_receiver) = if member.attached {
            let (&self_ty, parameters) = function_ty.parameters.split_first()?;
            if !receiver_ty.convert_implicit_to(self_ty, self) {
                return None;
            }
            (parameters, true)
        } else {
            (function_ty.parameters, false)
        };
        Some(CallableSignature {
            parameters,
            returns: function_ty.returns,
            param_source: function_ty
                .function_id
                .map(|id| CallableParamSource::Function { id, skips_receiver }),
        })
    }

    /// Returns the parameter names from a callable parameter source.
    pub fn callable_param_names(self, source: CallableParamSource) -> CallableParamNames {
        match source {
            CallableParamSource::Function { id, skips_receiver } => {
                let mut names = self.param_names(self.hir.function(id).parameters);
                if skips_receiver {
                    debug_assert!(!names.is_empty());
                    names.remove(0);
                }
                names
            }
            CallableParamSource::FunctionType(id) => match self.hir.variable(id).ty.kind {
                hir::TypeKind::Function(ty) => self.param_names(ty.parameters),
                _ => Default::default(),
            },
            CallableParamSource::Struct(id) => self.param_names(self.hir.strukt(id).fields),
            CallableParamSource::Event(id) => self.param_names(self.hir.event(id).parameters),
            CallableParamSource::Error(id) => self.param_names(self.hir.error(id).parameters),
            CallableParamSource::Builtin(Builtin::AbiDecode) => {
                [Some(sym::data), Some(sym::types)].into_iter().collect()
            }
            CallableParamSource::Builtin(_) => Default::default(),
        }
    }

    fn callable_param_source_for_fn(self, function_ty: &TyFn<'gcx>) -> Option<CallableParamSource> {
        function_ty.function_id.map(|id| {
            let declared_param_count = self.hir.function(id).parameters.len();
            let visible_param_count = function_ty.parameters.len();
            debug_assert!(
                declared_param_count == visible_param_count
                    || declared_param_count == visible_param_count + 1
            );
            CallableParamSource::Function {
                id,
                skips_receiver: declared_param_count == visible_param_count + 1,
            }
        })
    }

    fn struct_constructor_signature(self, ty: Ty<'gcx>) -> Option<CallableSignature<'gcx>> {
        let TyKind::Struct(id) = ty.kind else { return None };
        let parameters = self.mk_ty_iter(
            self.struct_field_types(id)
                .iter()
                .map(|&field_ty| field_ty.with_loc_if_ref(self, DataLocation::Memory)),
        );
        let returns = self.mk_ty_iter(std::iter::once(ty.with_loc(self, DataLocation::Memory)));
        Some(CallableSignature {
            parameters,
            returns,
            param_source: Some(CallableParamSource::Struct(id)),
        })
    }

    fn param_names(self, params: &[hir::VariableId]) -> CallableParamNames {
        params.iter().map(|&id| self.hir.variable(id).name.map(|i| i.name)).collect()
    }

    /// Returns the type of the given literal.
    pub fn type_of_lit(self, lit: &'gcx hir::Lit<'gcx>) -> Ty<'gcx> {
        match &lit.kind {
            solar_ast::LitKind::Str(_, s, _) => self.mk_ty_string_literal(s.as_byte_str()),
            solar_ast::LitKind::Number(int) => {
                let compatible_fixed_bytes = compatible_fixed_bytes_type(lit);
                self.mk_ty_int_literal_with_fixed_bytes(
                    false,
                    int.bit_len() as _,
                    compatible_fixed_bytes,
                )
                .unwrap_or_else(|| {
                    self.mk_ty_err(
                        self.dcx().emit_err(lit.span, "integer literal is greater than 2**256"),
                    )
                })
            }
            solar_ast::LitKind::Rational(_) => {
                let value = lit.symbol.as_str();
                if value.ends_with('_')
                    || value.contains("__")
                    || value.contains("._")
                    || value.contains("_.")
                    || value.contains("_e")
                    || value.contains("_E")
                    || value.contains("e_")
                    || value.contains("E_")
                {
                    self.mk_ty_misc_err()
                } else {
                    self.mk_ty_err(
                        self.dcx().emit_err(lit.span, "rational literals are not supported"),
                    )
                }
            }
            solar_ast::LitKind::Address(_) => self.types.address,
            solar_ast::LitKind::Bool(_) => self.types.bool,
            &solar_ast::LitKind::Err(guar) => self.mk_ty_err(guar),
        }
    }

    pub fn members_of(
        self,
        ty: Ty<'gcx>,
        source: hir::SourceId,
        contract: Option<hir::ContractId>,
    ) -> impl Iterator<Item = members::Member<'gcx>> + 'gcx {
        let native =
            self.native_members_in_context(ty, contract).unwrap_or_else(|| self.native_members(ty));
        let attached = self.attached_functions(ty, source, contract);
        native.iter().copied().chain(attached)
    }

    pub fn member_completions_of(
        self,
        ty: Ty<'gcx>,
        source: hir::SourceId,
        contract: Option<hir::ContractId>,
    ) -> impl Iterator<Item = MemberCompletion<'gcx>> + 'gcx {
        self.members_of(ty, source, contract).map(move |member| MemberCompletion {
            resolved: self.resolve_member_target(ty, member.name, member.res),
            member,
        })
    }

    pub(crate) fn resolve_member_target(
        self,
        receiver_ty: Ty<'gcx>,
        name: Symbol,
        res: Option<hir::Res>,
    ) -> Option<ResolvedMember> {
        if let Some(res) = res {
            let struct_id = match receiver_ty.kind {
                TyKind::Ref(inner, _) => {
                    if let TyKind::Struct(struct_id) = inner.kind {
                        Some(struct_id)
                    } else {
                        None
                    }
                }
                TyKind::Struct(struct_id) => Some(struct_id),
                _ => None,
            };
            if let Some(field_id) = res.as_variable()
                && let Some(struct_id) = struct_id
                && self.hir.variable(field_id).parent == Some(hir::ItemId::Struct(struct_id))
                && let Some(field_index) =
                    self.hir.strukt(struct_id).fields.iter().position(|&id| id == field_id)
            {
                return Some(ResolvedMember::StructField { struct_id, field_index });
            }
            return Some(ResolvedMember::Res(res));
        }

        match receiver_ty.kind {
            TyKind::Ref(inner, _) => {
                let TyKind::Struct(struct_id) = inner.kind else { return None };
                let field_index = self.struct_field_index(struct_id, name)?;
                Some(ResolvedMember::StructField { struct_id, field_index })
            }
            TyKind::Struct(struct_id) => {
                let field_index = self.struct_field_index(struct_id, name)?;
                Some(ResolvedMember::StructField { struct_id, field_index })
            }
            TyKind::Type(ty) => {
                let TyKind::Enum(enum_id) = ty.kind else { return None };
                let variant_index = self
                    .hir
                    .enumm(enum_id)
                    .variants
                    .iter()
                    .position(|variant| variant.name == name)?;
                Some(ResolvedMember::EnumVariant { enum_id, variant_index })
            }
            _ => None,
        }
    }

    fn struct_field_index(self, struct_id: hir::StructId, name: Symbol) -> Option<usize> {
        self.hir.strukt(struct_id).fields.iter().position(|&field_id| {
            self.hir.variable(field_id).name.is_some_and(|field| field.name == name)
        })
    }

    fn native_members_in_context(
        self,
        ty: Ty<'gcx>,
        current_contract: Option<hir::ContractId>,
    ) -> Option<members::MemberList<'gcx>> {
        let current_contract = current_contract?;
        match ty.kind {
            TyKind::Type(ty) => {
                let TyKind::Contract(id) = ty.kind else { return None };
                let contract = self.hir.contract(id);
                if contract.kind.is_library()
                    || !self.hir.contract(current_contract).linearized_bases.contains(&id)
                {
                    return None;
                }
                Some(self.contract_type_members_in_context((id, current_contract)))
            }
            TyKind::Fn(f) if f.kind == TyFnKind::Internal => {
                let id = f.function_id?;
                Some(self.internal_function_members_in_context((id, current_contract)))
            }
            _ => None,
        }
    }

    pub(crate) fn for_each_user_operator(
        self,
        ty: Ty<'gcx>,
        source: hir::SourceId,
        contract: Option<hir::ContractId>,
        op: UserDefinableOperator,
        unary: bool,
        f: &mut dyn FnMut(hir::FunctionId),
    ) {
        let TyKind::Udvt(_, user_ty) = ty.peel_refs().kind else {
            return;
        };
        let ty = self.type_of_item(user_ty.into());
        let mut seen = DenseBitSet::new_empty(self.hir.function_ids().len());
        self.for_each_using_directive_for_type(ty, source, contract, &mut |using| {
            for entry in using.entries {
                if entry.operator == Some(op)
                    && let hir::UsingEntryKind::Functions(candidates) = entry.kind
                {
                    for &function_id in candidates {
                        if let TyKind::Fn(function_ty) = self.type_of_item(function_id.into()).kind
                            && function_ty.parameters.len() == if unary { 1 } else { 2 }
                            && function_ty.parameters.first().copied() == Some(ty)
                            && seen.insert(function_id)
                        {
                            f(function_id);
                        }
                    }
                }
            }
        });
    }

    fn attached_functions(
        self,
        ty: Ty<'gcx>,
        source: hir::SourceId,
        contract: Option<hir::ContractId>,
    ) -> Vec<members::Member<'gcx>> {
        let mut members = Vec::new();
        let mut seen = FxHashSet::default();
        self.for_each_using_directive_for_type(ty, source, contract, &mut |using| {
            for entry in using.entries {
                if entry.operator.is_some() {
                    continue;
                }
                match entry.kind {
                    hir::UsingEntryKind::Library(library) => {
                        for function in self.hir.contract(library).functions() {
                            let f = self.hir.function(function);
                            if !f.is_ordinary()
                                || f.parameters.is_empty()
                                || f.visibility == Visibility::Private
                            {
                                continue;
                            }
                            self.add_attached_function(ty, function, None, &mut seen, &mut members);
                        }
                    }
                    hir::UsingEntryKind::Functions(functions) => {
                        for &function in functions {
                            let name = entry.name.unwrap_or_else(|| self.item_name(function).name);
                            self.add_attached_function(
                                ty,
                                function,
                                Some(name),
                                &mut seen,
                                &mut members,
                            );
                        }
                    }
                    hir::UsingEntryKind::Err(_) => {}
                }
            }
        });
        members
    }

    fn add_attached_function(
        self,
        ty: Ty<'gcx>,
        function: hir::FunctionId,
        name: Option<Symbol>,
        seen: &mut FxHashSet<(Symbol, hir::FunctionId)>,
        members: &mut Vec<members::Member<'gcx>>,
    ) {
        let function_item = self.hir.function(function);
        let fn_ty = self.type_of_item(function.into());
        let fn_ty =
            if function_item.contract.is_some_and(|id| self.hir.contract(id).kind.is_library())
                && function_item.visibility >= Visibility::Public
            {
                fn_ty.as_externally_callable_function(true, self)
            } else {
                fn_ty
            }
            .as_attached_function(self);
        if let TyKind::Fn(function_ty) = fn_ty.kind
            && let Some(&self_ty) = function_ty.parameters.first()
            && ty.convert_implicit_to(self_ty, self)
            && let name = name.unwrap_or_else(|| self.item_name(function).name)
            && seen.insert((name, function))
        {
            members.push(members::Member::with_attached_function(name, fn_ty, function));
        }
    }

    fn for_each_using_directive_for_type(
        self,
        ty: Ty<'gcx>,
        source: hir::SourceId,
        contract: Option<hir::ContractId>,
        f: &mut dyn FnMut(&'gcx hir::UsingDirective<'gcx>),
    ) {
        let mut check = |usings: &'gcx [hir::UsingDirective<'gcx>], only_global: bool| {
            for using in usings {
                if self.using_directive_applies(using, ty, only_global) {
                    f(using);
                }
            }
        };

        if let Some(contract) = contract {
            check(self.hir.contract(contract).usings, false);
        }
        check(self.hir.source(source).usings, false);

        if let Some(type_source) = ty.item_source(self)
            && type_source != source
        {
            check(self.hir.source(type_source).usings, true);
        }
    }

    fn using_directive_applies(
        self,
        using: &'gcx hir::UsingDirective<'gcx>,
        ty: Ty<'gcx>,
        only_global: bool,
    ) -> bool {
        if only_global && !(using.global && using.ty.is_some()) {
            return false;
        }
        let Some(using_ty) = self.type_of_using_directive(using) else {
            // For `*`.
            return true;
        };
        let loc = ty.loc().unwrap_or(DataLocation::Storage);
        using_directive_ty_matches(ty, using_ty.with_loc_if_ref(self, loc))
    }
}

fn using_directive_ty_matches(ty: Ty<'_>, using_ty: Ty<'_>) -> bool {
    if ty == using_ty {
        return true;
    }
    // HACK: allow attached functions to be called on function declarations. Function type equality
    // also checks declaration IDs, so this is a quick way to make it work for now.
    if let (TyKind::Fn(a), TyKind::Fn(b)) = (ty.kind, using_ty.kind) {
        return a.kind == b.kind
            && a.parameters == b.parameters
            && a.returns == b.returns
            && a.state_mutability == b.state_mutability
            && a.attached == b.attached;
    }
    false
}

fn compatible_fixed_bytes_type(lit: &hir::Lit<'_>) -> Option<TypeSize> {
    let solar_ast::LitKind::Number(int) = lit.kind else { return None };
    if int.is_zero() {
        return Some(TypeSize::ZERO);
    }

    let hex = lit.symbol.as_str().strip_prefix("0x")?;
    let digit_count = hex.bytes().filter(|&b| b != b'_').count();
    if digit_count % 2 == 0 {
        TypeSize::try_new_fb_bytes((digit_count / 2).try_into().ok()?)
    } else {
        None
    }
}

fn fn_state_mutability(kind: TyFnKind, state_mutability: StateMutability) -> StateMutability {
    if kind == TyFnKind::Internal && state_mutability == StateMutability::Payable {
        StateMutability::NonPayable
    } else {
        state_mutability
    }
}

macro_rules! cached_key_type {
    ($key_type:ty) => {
        $key_type
    };
    ($key_type:ty, $cache_key_type:ty) => {
        $cache_key_type
    };
}

macro_rules! cached_key_expr {
    ($key:expr) => {
        $key
    };
    ($key:expr, $cache_key:expr) => {
        $cache_key
    };
}

macro_rules! cached {
    ($($(#[$attr:meta])* $vis:vis fn $name:ident($gcx:ident: _, $key:ident : $key_type:ty $(,)?) $(cached_by($cache_key_type:ty, $cache_key:expr))? -> $value:ty $imp:block)*) => {
        #[derive(Default)]
        struct Cache<'gcx> {
            function_reference_index: OnceLock<&'gcx hir::FunctionReferenceIndex<'gcx>>,
            $(
                $name: FxOnceMap<cached_key_type!($key_type $(, $cache_key_type)?), $value>,
            )*
        }

        impl<'gcx> Gcx<'gcx> {
            $(
                $(#[$attr])*
                $vis fn $name(self, $key: $key_type) -> $value {
                    let cache_key = cached_key_expr!($key $(, $cache_key)?);
                    #[cfg(false)]
                    let _guard = log_cache_query(stringify!($name), &cache_key);
                    #[cfg(false)]
                    let mut hit = true;
                    let r = cache_insert(&self.cache.$name, cache_key, |_| {
                        #[cfg(false)]
                        {
                            hit = false;
                        }
                        let $gcx = self;
                        $imp
                    });
                    #[cfg(false)]
                    log_cache_query_result(&r, hit);
                    r
                }
            )*
        }
    };
}

cached! {
/// Returns the [ERC-165] interface ID of the given contract.
///
/// This is the XOR of the selectors of all function selectors in the interface.
///
/// The solc implementation excludes inheritance: <https://github.com/argotorg/solidity/blob/ad2644c52b3afbe80801322c5fe44edb59383500/libsolidity/ast/AST.cpp#L310-L316>
///
/// See [ERC-165] for more details.
///
/// [ERC-165]: https://eips.ethereum.org/EIPS/eip-165
pub fn interface_id(gcx: _, id: hir::ContractId) -> Selector {
    let kind = gcx.hir.contract(id).kind;
    assert!(kind.is_interface(), "{kind} {id:?} is not an interface");
    let selectors = gcx.interface_functions(id).own().iter().map(|f| f.selector);
    selectors.fold(Selector::ZERO, std::ops::BitXor::bitxor)
}

/// Returns all the exported functions of the given contract.
///
/// The contract doesn't have to be an interface.
pub fn interface_functions(gcx: _, id: hir::ContractId) -> InterfaceFunctions<'gcx> {
    let c = gcx.hir.contract(id);
    let mut inheritance_start = None;
    let mut signatures_seen = FxHashSet::default();
    let mut hash_collisions = FxHashMap::default();
    let functions = c.linearized_bases.iter().flat_map(|&base| {
        let b = gcx.hir.contract(base);
        let functions =
            b.functions().filter(|&f| gcx.hir.function(f).is_part_of_external_interface());
        if base == id {
            assert!(inheritance_start.is_none(), "duplicate self ID in linearized_bases");
            inheritance_start = Some(functions.clone().count());
        }
        functions
    }).filter_map(|f_id| {
        let f = gcx.hir.function(f_id);
        let TyKind::Fn(fn_ty) = gcx.type_of_item(f_id.into()).kind else { unreachable!() };
        let ty = gcx
            .mk_ty_fn(TyFn {
                kind: TyFnKind::External,
                parameters: fn_ty.parameters,
                returns: fn_ty.returns,
                state_mutability: f.state_mutability,
                function_id: fn_ty.function_id,
                attached: false,
            })
            .as_externally_callable_function(false, gcx);
        let TyKind::Fn(ty_f) = ty.kind else { unreachable!() };
        let mut result = Ok(());
        for (var_id, ty) in f.variables().zip(ty_f.tys()) {
            if let Err(guar) = ty.error_reported() {
                result = Err(guar);
                continue;
            }
            if !ty.can_be_exported(gcx) {
                // TODO: implement `interfaceType`
                if c.kind.is_library() {
                    result = Err(ErrorGuaranteed::new_unchecked());
                    continue;
                }

                let kind = f.description();
                let msg = if ty.has_mapping(gcx) {
                    format!("types containing mappings cannot be parameter or return types of public {kind}s")
                } else if ty.is_recursive(gcx) {
                    format!("recursive types cannot be parameter or return types of public {kind}s")
                } else if ty.has_internal_function() {
                    format!("types containing internal function pointers cannot be parameter or return types of public {kind}s")
                } else {
                    format!("this type cannot be parameter or return type of a public {kind}")
                };
                let span = gcx.hir.variable(var_id).ty.span;
                result = Err(gcx.dcx().emit_err(span, msg));
            }
        }
        if result.is_err() {
            return None;
        }

        // Virtual functions or ones with the same function parameter types are checked separately,
        // skip them here to avoid reporting them as selector hash collision errors below.
        let hash = gcx.item_selector(f_id.into());
        let selector: Selector = hash[..4].try_into().unwrap();
        if !signatures_seen.insert(hash) {
            return None;
        }

        // Check for selector hash collisions.
        if let Some(prev) = hash_collisions.insert(selector, f_id) {
            let f2 = gcx.hir.function(prev);
            let msg = "function signature hash collision";
            let full_note = format!(
                "the function signatures `{}` and `{}` produce the same 4-byte selector `{selector}`",
                gcx.item_signature(f_id.into()),
                gcx.item_signature(prev.into()),
            );
            gcx.dcx().err(msg).span(c.name.span).span_note(f.span, "first function").span_note(f2.span, "second function").note(full_note).emit();
        }

        Some(InterfaceFunction { selector, id: f_id, ty })
    });
    let functions = gcx.bump().alloc_from_iter(functions);
    trace!("{}.interfaceFunctions.len() = {}", gcx.contract_fully_qualified_name(id), functions.len());
    let inheritance_start = inheritance_start.expect("linearized_bases did not contain self ID");
    InterfaceFunctions { functions, inheritance_start }
}

/// Returns the effective runtime entry points of a deployed contract.
///
/// The result contains the most-derived public/external implementation for each ABI signature,
/// followed by the most-derived fallback and receive functions when present. Constructors,
/// modifiers, and shadowed base implementations are excluded.
pub fn runtime_entry_points(gcx: _, id: hir::ContractId) -> &'gcx [hir::FunctionId] {
    let contract = gcx.hir.contract(id);
    if contract.linearization_failed() {
        return &[];
    }

    let mut seen = FxHashSet::default();
    let mut signatures = FxHashSet::default();
    let mut functions = SmallVec::<[hir::FunctionId; 16]>::new();
    for &base in contract.linearized_bases {
        for function in gcx.hir.contract(base).functions() {
            let declaration = gcx.hir.function(function);
            if !declaration.is_ordinary()
                || !matches!(declaration.visibility, Visibility::Public | Visibility::External)
                || declaration.name.is_none()
            {
                continue;
            }
            let signature = gcx.item_signature(hir::ItemId::Function(function));
            if signatures.insert(signature) {
                let function = gcx.function_in_contract(function, id);
                if seen.insert(function) {
                    functions.push(function);
                }
            }
        }
    }
    for function in [
        contract
            .linearized_bases
            .iter()
            .find_map(|&base| gcx.hir.contract(base).fallback),
        contract
            .linearized_bases
            .iter()
            .find_map(|&base| gcx.hir.contract(base).receive),
    ]
    .into_iter()
    .flatten()
    {
        let function = gcx.function_in_contract(function, id);
        if seen.insert(function) {
            functions.push(function);
        }
    }
    gcx.bump().alloc_slice_copy(&functions)
}

/// Returns the mutable contract state variables reachable in `id`'s runtime state layout.
///
/// Variables from inherited contracts are included. Constants and immutables are excluded because
/// runtime calls cannot update them.
pub fn contract_mutable_state_variables(
    gcx: _, id: hir::ContractId
) -> &'gcx [hir::VariableId] {
    let contract = gcx.hir.contract(id);
    let mut variables = SmallVec::<[hir::VariableId; 16]>::new();
    if contract.linearized_bases.is_empty() {
        variables.extend(gcx.hir.contract(id).variables().filter(|&variable| {
            let variable = gcx.hir.variable(variable);
            variable.kind.is_state() && !variable.is_constant() && !variable.is_immutable()
        }));
    } else {
        for &base in contract.linearized_bases {
            variables.extend(gcx.hir.contract(base).variables().filter(|&variable| {
                let variable = gcx.hir.variable(variable);
                variable.kind.is_state() && !variable.is_constant() && !variable.is_immutable()
            }));
        }
    }
    gcx.bump().alloc_slice_copy(&variables)
}

pub(crate) fn base_override_functions(
    gcx: _,
    proxy: crate::typeck::override_checker::OverrideProxy
) -> &'gcx [crate::typeck::override_checker::OverrideProxy] {
    crate::typeck::override_checker::base_override_functions(gcx, proxy)
}

/// Returns the resolved NatSpec doc comments for the given doc ID.
pub fn natspec_doc_comments(gcx: _, id: hir::DocId) -> &'gcx [hir::NatSpecItem] {
    crate::natspec::resolve_doc_comments(gcx, id)
}

/// Resolves a contract name within a source's scope for NatSpec `@inheritdoc`.
pub(crate) fn natspec_contract_in_source(
    gcx: _,
    key: NatSpecContractKey
) -> Option<hir::ContractId> {
    let (name, source_id) = key;
    gcx.symbol_resolver.source_scopes[source_id]
        .resolve(solar_interface::Ident { name, span: Span::DUMMY })
        .and_then(|decls| {
            decls.iter().find_map(|decl| match decl.res {
                hir::Res::Item(hir::ItemId::Contract(id)) => Some(id),
                _ => None,
            })
        })
}

/// Returns the ABI signature of the given item. Only accepts functions, errors, and events.
pub fn item_signature(gcx: _, id: hir::ItemId) -> &'gcx str {
    let name = gcx.item_name(id);
    let tys = gcx.item_parameter_types(id);
    let in_library =
        gcx.hir.item(id).contract().is_some_and(|c| gcx.hir.contract(c).kind.is_library());
    gcx.bump().alloc_str(&gcx.mk_abi_signature(name.as_str(), tys.iter().copied(), in_library))
}

pub(crate) fn item_selector(gcx: _, id: hir::ItemId) -> B256 {
    keccak256(gcx.item_signature(id))
}

/// Returns the type of the given builtin.
pub fn type_of_builtin(gcx: _, builtin: Builtin) -> Ty<'gcx> {
    builtin.ty_impl(gcx)
}

fn type_of_using_directive_cached(gcx: _, key: UsingDirectiveKey) -> Option<Ty<'gcx>> {
    // HIR nodes are arena-allocated for the lifetime of `GlobalCtxt`.
    let using = unsafe { &*(key as *const hir::UsingDirective<'gcx>) };
    using.ty.as_ref().map(|ty| gcx.type_of_hir_ty(ty))
}

/// Returns the type of the given item.
pub fn type_of_item(gcx: _, id: hir::ItemId) -> Ty<'gcx> {
    let kind = match id {
        hir::ItemId::Contract(id) => TyKind::Contract(id),
        hir::ItemId::Function(id) => {
            let f = gcx.hir.function(id);
            return gcx.mk_ty_fn(TyFn {
                kind: TyFnKind::Internal,
                parameters: gcx.mk_item_tys(f.parameters),
                returns: gcx.mk_item_tys(f.returns),
                state_mutability: fn_state_mutability(TyFnKind::Internal, f.state_mutability),
                function_id: Some(id),
                attached: false,
            });
        }
        hir::ItemId::Variable(id) => {
            let var = gcx.hir.variable(id);
            let ty = gcx.type_of_hir_ty(&var.ty);
            return var_type(gcx, var, ty);
        }
        hir::ItemId::Struct(id) => TyKind::Struct(id),
        hir::ItemId::Enum(id) => TyKind::Enum(id),
        hir::ItemId::Udvt(id) => {
            let udvt = gcx.hir.udvt(id);
            if udvt.ty.kind.is_elementary()
                && let ty = gcx.type_of_hir_ty(&udvt.ty)
                && ty.is_value_type()
            {
                TyKind::Udvt(ty, id)
            } else {
                let msg = "the underlying type of UDVTs must be an elementary value type";
                return gcx.mk_ty_err(gcx.dcx().emit_err(udvt.ty.span, msg));
            }
        }
        hir::ItemId::Error(id) => {
            TyKind::Error(gcx.mk_item_tys(gcx.hir.error(id).parameters), id)
        }
        hir::ItemId::Event(id) => {
            TyKind::Event(gcx.mk_item_tys(gcx.hir.event(id).parameters), id)
        }
    };
    gcx.mk_ty(kind)
}

/// Returns the types of the fields of the given struct.
pub fn struct_field_types(gcx: _, id: hir::StructId) -> &'gcx [Ty<'gcx>] {
    gcx.mk_ty_iter(gcx.hir.strukt(id).fields.iter().map(|&f| gcx.type_of_item(f.into())))
}

/// Returns the recursiveness of the given struct.
pub fn struct_recursiveness(gcx: _, id: hir::StructId) -> Recursiveness {
    use solar_data_structures::cycle::*;

    let r = CycleDetector::detect(gcx, id, |gcx, cd, id| {
        let s = gcx.hir.strukt(id);

        if cd.depth() >= 256 {
            let guar = gcx.dcx().emit_err(s.span, "struct is too deeply nested");
            return CycleDetectorResult::Break(Either::Left(guar));
        }

        for &field_id in s.fields {
            let field = gcx.hir.variable(field_id);
            let mut check = |ty: &hir::Type<'_>, dynamic: bool| {
                if let hir::TypeKind::Custom(hir::ItemId::Struct(other)) = ty.kind {
                    match cd.run(other) {
                        CycleDetectorResult::Continue => {}
                        CycleDetectorResult::Cycle(_) if dynamic => {
                            return CycleDetectorResult::Break(Either::Right(()));
                        }
                        r => return r,
                    }
                }
                CycleDetectorResult::Continue
            };
            let mut dynamic = false;
            let mut ty = &field.ty;
            while let hir::TypeKind::Array(array) = ty.kind {
                if array.size.is_none() {
                    dynamic = true;
                }
                ty = &array.element;
            }
            cdr_try!(check(ty, dynamic));
            if let ControlFlow::Break(r) = field.ty.visit(&gcx.hir, &mut |ty| check(ty, true).to_controlflow()) {
                return r;
            }
        }

        CycleDetectorResult::Continue
    });
    match r {
        CycleDetectorResult::Continue => Recursiveness::None,
        CycleDetectorResult::Break(Either::Left(guar)) => Recursiveness::Infinite(guar),
        CycleDetectorResult::Break(Either::Right(())) => Recursiveness::Recursive,
        CycleDetectorResult::Cycle(id) => Recursiveness::Infinite(
            gcx.dcx().emit_err(gcx.item_span(id), "recursive struct definition")
        ),
    }
}

fn native_members(gcx: _, ty: Ty<'gcx>) -> members::MemberList<'gcx> {
    members::native_members(gcx, ty)
}

fn contract_type_members_in_context(
    gcx: _,
    key: (hir::ContractId, hir::ContractId)
) -> members::MemberList<'gcx> {
    let (id, current_contract) = key;
    members::contract_type_members_in_context(gcx, id, current_contract)
}

fn internal_function_members_in_context(
    gcx: _,
    key: (hir::FunctionId, hir::ContractId)
) -> members::MemberList<'gcx> {
    let (id, current_contract) = key;
    gcx.bump().alloc_vec(members::internal_function_members_in_context(gcx, id, current_contract))
}

pub(crate) fn eval_const_value_result(gcx: _, expr: &hir::Expr<'_>)
    cached_by(hir::ExprId, expr.id) -> &'gcx crate::eval::EvalResult
{
    gcx.alloc(crate::eval::eval_const(gcx, expr))
}

} // cached!

fn var_type<'gcx>(gcx: Gcx<'gcx>, var: &'gcx hir::Variable<'gcx>, ty: Ty<'gcx>) -> Ty<'gcx> {
    use hir::DataLocation::*;

    // https://github.com/argotorg/solidity/blob/48d40d5eaf97c835cf55896a7a161eedc57c57f9/libsolidity/ast/AST.cpp#L820
    let mut has_reference_or_mapping_type_slot = None;
    let mut has_reference_or_mapping_type = || {
        *has_reference_or_mapping_type_slot
            .get_or_insert_with(|| ty.is_reference_type() || ty.has_mapping(gcx))
    };

    let mut func_vis = None;
    let mut locs;
    let allowed: &[_] = if var.is_state_variable() {
        &[None, Some(Transient)]
    } else if !has_reference_or_mapping_type() || var.is_event_or_error_parameter() {
        &[None]
    } else if var.is_callable_or_catch_parameter() {
        locs = SmallVec::<[_; 3]>::new();
        locs.push(Some(Memory));
        let mut is_constructor_parameter = false;
        if let Some(hir::ItemId::Function(f)) = var.parent {
            let f = gcx.hir.function(f);
            is_constructor_parameter = f.kind.is_constructor();
            if !var.is_try_catch_parameter() && !is_constructor_parameter {
                func_vis = Some(f.visibility);
            }
            if is_constructor_parameter
                || f.visibility <= hir::Visibility::Internal
                || f.contract.is_some_and(|c| gcx.hir.contract(c).kind.is_library())
            {
                locs.push(Some(Storage));
            }
        }
        if !var.is_try_catch_parameter() && !is_constructor_parameter {
            locs.push(Some(Calldata));
        }
        &locs
    } else if var.is_local_variable() {
        &[Some(Memory), Some(Storage), Some(Calldata)]
    } else {
        &[None]
    };

    let mut var_loc = var.data_location;
    if !allowed.contains(&var_loc) {
        if !ty.references_error() {
            let msg = if !has_reference_or_mapping_type() {
                "data location can only be specified for array, struct or mapping types".to_string()
            } else if let Some(var_loc) = var_loc {
                format!("invalid data location `{var_loc}`")
            } else {
                "expected data location".to_string()
            };
            let mut err = gcx.dcx().err(msg).span(var.span);
            if has_reference_or_mapping_type() {
                let note = format!(
                    "data location must be {expected} for {vis}{descr}{got}",
                    expected = or_list(
                        allowed.iter().map(|d| format!("`{}`", DataLocation::opt_to_str(*d)))
                    ),
                    vis = if let Some(vis) = func_vis { format!("{vis} ") } else { String::new() },
                    descr = var.description(),
                    got = if let Some(var_loc) = var_loc {
                        format!(", but got `{var_loc}`")
                    } else {
                        String::new()
                    },
                );
                err = err.note(note);
            }
            err.emit();
        }
        var_loc = allowed[0];
    }

    let ty_loc = if var.is_event_or_error_parameter() || var.is_file_level_variable() {
        Memory
    } else if var.is_state_variable() {
        let mut_specified = var.mutability.is_some();
        match var_loc {
            None => {
                if mut_specified {
                    Memory
                } else {
                    Storage
                }
            }
            Some(Transient) => {
                if mut_specified {
                    let msg = "transient cannot be used as data location for constant or immutable variables";
                    gcx.dcx().emit_err(var.span, msg);
                }
                if var.initializer.is_some() {
                    let msg =
                        "initialization of transient storage state variables is not supported";
                    gcx.dcx().emit_err(var.span, msg);
                }
                Transient
            }
            Some(_) => unreachable!(),
        }
    } else if var.is_struct_member() {
        Storage
    } else {
        match var_loc {
            Some(loc @ (Memory | Storage | Calldata)) => loc,
            Some(Transient) => unimplemented!(),
            None => {
                debug_assert!(!has_reference_or_mapping_type(), "data location not properly set");
                Memory
            }
        }
    };

    ty.with_loc_if_ref(gcx, ty_loc)
}

/// True if referencing the item returns its type directly rather than wrapped in Type().
fn is_value_ns(id: hir::ItemId) -> bool {
    matches!(
        id,
        hir::ItemId::Function(_)
            | hir::ItemId::Variable(_)
            | hir::ItemId::Error(_)
            | hir::ItemId::Event(_)
    )
}

/// `OnceMap::insert` but with `Copy` keys and values.
#[inline]
fn cache_insert<K, V>(map: &FxOnceMap<K, V>, key: K, make_val: impl FnOnce(&K) -> V) -> V
where
    K: Copy + Eq + Hash,
    V: Copy,
{
    map.map_insert(key, make_val, cache_insert_with_result)
}

#[inline]
fn cache_insert_with_result<K, V: Copy>(_: &K, v: &V) -> V {
    *v
}

#[cfg(false)]
fn log_cache_query(name: &str, key: &dyn fmt::Debug) -> tracing::span::EnteredSpan {
    let guard = trace_span!("query", %name, ?key).entered();
    trace!("entered");
    guard
}

#[cfg(false)]
fn log_cache_query_result(result: &dyn fmt::Debug, hit: bool) {
    trace!(?result, hit);
}

impl<'gcx> Gcx<'gcx> {
    /// Returns the cached compilation-wide reverse-reference index for function declarations.
    pub fn function_reference_index(self) -> &'gcx hir::FunctionReferenceIndex<'gcx> {
        self.cache.function_reference_index.get_or_init(|| {
            self.bump().alloc(hir::references::build_function_reference_index(self))
        })
    }

    /// Returns the finite declarations which an unresolved internal function-pointer call may
    /// target.
    ///
    /// The compilation-wide value index follows local bindings, ternaries, and arguments passed
    /// to statically resolved internal calls. [`hir::FunctionValueTargets::may_be_unknown`]
    /// distinguishes a complete finite set from one which also needs an opaque branch.
    pub fn indirect_internal_call_targets(
        self,
        call: &hir::Expr<'gcx>,
    ) -> hir::FunctionValueTargets {
        if !self.call_info(call).is_some_and(CallInfo::is_indirect_internal) {
            return hir::FunctionValueTargets::unknown();
        }
        let hir::ExprKind::Call(callee, ..) = call.peel_parens().kind else {
            return hir::FunctionValueTargets::unknown();
        };
        self.function_reference_index().possible_value_targets(self, callee)
    }
}

#[cfg(test)]
mod call_arg_tests {
    use super::*;
    use crate::{Compiler, hir::Visit};
    use solar_data_structures::Never;
    use solar_interface::{Session, config::CompileOpts};
    use std::{collections::BTreeMap, ops::ControlFlow, path::PathBuf};

    struct CallCollector<'hir> {
        hir: &'hir hir::Hir<'hir>,
        calls: Vec<&'hir hir::Expr<'hir>>,
    }

    impl<'hir> Visit<'hir> for CallCollector<'hir> {
        type BreakValue = Never;

        fn hir(&self) -> &'hir hir::Hir<'hir> {
            self.hir
        }

        fn visit_expr(&mut self, expr: &'hir hir::Expr<'hir>) -> ControlFlow<Self::BreakValue> {
            if matches!(expr.kind, hir::ExprKind::Call(..)) {
                self.calls.push(expr);
            }
            self.walk_expr(expr)
        }
    }

    fn call_arguments(source: &str, expect_errors: bool) -> BTreeMap<String, Vec<Option<String>>> {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);

        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            let file =
                c.sess().source_map().new_source_file(PathBuf::from("test.sol"), source).unwrap();
            pcx.add_file(file);
            pcx.parse();

            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
        });
        assert_eq!(compiler.sess().dcx.has_errors().is_err(), expect_errors);

        compiler.enter(|c| {
            let gcx = c.gcx();
            let mut visitor = CallCollector { hir: &gcx.hir, calls: Vec::new() };
            for source in gcx.hir.source_ids() {
                let _ = visitor.visit_nested_source(source);
            }
            visitor
                .calls
                .into_iter()
                .map(|call| {
                    let source_map = gcx.sess.source_map();
                    let call_source = source_map.span_to_snippet(call.span).unwrap();
                    let arguments = (0..3)
                        .map(|index| {
                            gcx.call_arg(call, index)
                                .map(|arg| source_map.span_to_snippet(arg.span).unwrap())
                        })
                        .collect();
                    (call_source, arguments)
                })
                .collect()
        })
    }

    #[test]
    fn call_arg_uses_visible_parameter_order() {
        let calls = call_arguments(
            r#"
struct CollisionHolder {
    function(bool fieldFlag, address fieldAccount) internal pure callback;
}

library L {
    function attached(uint256 self, uint256 first, uint256 second)
        internal
        pure
        returns (uint256)
    {
        return self + first + second;
    }

    function callback(
        CollisionHolder memory self,
        uint256 attachedFirst,
        address attachedAccount
    ) internal pure {
        self;
        attachedFirst;
        attachedAccount;
    }
}

contract D {
    constructor(uint256 first, uint256 second) {
        first;
        second;
    }
}

contract C {
    using L for uint256;
    using L for CollisionHolder;

    struct Pair { uint256 first; uint256 second; }
    event E(uint256 first, uint256 second);
    error Err(uint256 first, uint256 second);

    function target(uint256 first, uint256 second) internal pure {}
    function fieldTarget(bool fieldFlag, address fieldAccount) internal pure {
        fieldFlag;
        fieldAccount;
    }
    function overloaded(uint256 first, uint256 second) internal pure {}
    function overloaded(address recipient, uint256 amount) internal pure {}

    function calls(uint256 receiver) external {
        target(11, 22);
        target({second: 22, first: 11});
        receiver.attached({second: 22, first: 11});
        Pair memory pair = Pair({second: 22, first: 11});
        D created = new D(11, 22);
        emit E({second: 22, first: 11});
        overloaded({amount: 22, recipient: address(11)});
        abi.encode(11, 22, 33);
        CollisionHolder memory collision =
            CollisionHolder({callback: fieldTarget});
        collision.callback({attachedAccount: address(44), attachedFirst: 33});
        pair;
        created;
        collision;
    }

    function fail() external pure {
        revert Err({second: 22, first: 11});
    }

    function decode(bytes memory raw) external pure returns (uint256) {
        return abi.decode({types: (uint256), data: raw});
    }
}
"#,
            false,
        );

        assert_eq!(calls["target(11, 22)"], [Some("11".into()), Some("22".into()), None]);
        assert_eq!(
            calls["target({second: 22, first: 11})"],
            [Some("11".into()), Some("22".into()), None]
        );
        assert_eq!(
            calls["receiver.attached({second: 22, first: 11})"],
            [Some("11".into()), Some("22".into()), None]
        );
        assert_eq!(
            calls["Pair({second: 22, first: 11})"],
            [Some("11".into()), Some("22".into()), None]
        );
        assert_eq!(calls["new D(11, 22)"], [Some("11".into()), Some("22".into()), None]);
        assert_eq!(
            calls["emit E({second: 22, first: 11});"],
            [Some("11".into()), Some("22".into()), None]
        );
        assert_eq!(
            calls["revert Err({second: 22, first: 11});"],
            [Some("11".into()), Some("22".into()), None]
        );
        assert_eq!(
            calls["overloaded({amount: 22, recipient: address(11)})"],
            [Some("address(11)".into()), Some("22".into()), None]
        );
        assert_eq!(calls["address(11)"], [Some("11".into()), None, None]);
        assert_eq!(
            calls["abi.encode(11, 22, 33)"],
            [Some("11".into()), Some("22".into()), Some("33".into())]
        );
        assert_eq!(
            calls["abi.decode({types: (uint256), data: raw})"],
            [Some("raw".into()), Some("(uint256)".into()), None]
        );
        assert_eq!(
            calls["collision.callback({attachedAccount: address(44), attachedFirst: 33})"],
            [Some("33".into()), Some("address(44)".into()), None]
        );
    }

    #[test]
    fn call_arg_handles_error_recovery() {
        let calls = call_arguments(
            r#"
contract C {
    struct Holder {
        function(uint256 first, uint256 second) internal pure callback;
    }

    function ambiguous(uint256 value) internal pure {}
    function ambiguous(int256 value) internal pure {}
    function target(uint256 first, uint256 second) internal pure {}

    function calls() external pure {
        ambiguous({value: 1});
        ambiguous(1);
        abi.encode({value: 1});
        uint256 notCallable = 1;
        notCallable(1);
        new D({second: 22, first: 11});
        function(uint256 first, uint256 second) internal pure callback = target;
        callback({second: 22, first: 11});
        Holder memory holder = Holder({callback: target});
        holder.callback({second: 44, first: 33});
    }
}

contract D {
    constructor(uint256 first, uint256 second) {
        first;
        second;
    }
}
"#,
            true,
        );

        assert_eq!(calls["ambiguous({value: 1})"], [None, None, None]);
        assert_eq!(calls["ambiguous(1)"], [None, None, None]);
        assert_eq!(calls["abi.encode({value: 1})"], [None, None, None]);
        assert_eq!(calls["notCallable(1)"], [None, None, None]);
        assert_eq!(
            calls["new D({second: 22, first: 11})"],
            [Some("11".into()), Some("22".into()), None]
        );
        assert_eq!(
            calls["callback({second: 22, first: 11})"],
            [Some("11".into()), Some("22".into()), None]
        );
        assert_eq!(
            calls["holder.callback({second: 44, first: 33})"],
            [Some("33".into()), Some("44".into()), None]
        );
    }
}
