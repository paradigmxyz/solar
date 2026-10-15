//! Constant evaluation.
//!
//! This evaluates side-effect free HIR expressions to typed values, like rustc's const eval does
//! for `const` items, but without an interpreter: it folds the expressions and builtins a constant
//! initializer can contain and never calls user code.
//!
//! # Modes
//!
//! Type checking needs array lengths before it knows the types of expressions, so evaluation has
//! two modes, each with its own memoized results:
//!
//! - [`Gcx::early_const_eval`] evaluates what solc's `ConstantEvaluator` evaluates: number and
//!   string literals, constants of integer, string and `bytes` types named by identifiers, unary
//!   and binary arithmetic, and `erc7201` of a string. It only uses name resolution and never
//!   reaches a type, so type checking uses it for array lengths and literal arithmetic. The checks
//!   after type checking use it for base slots and for what solc checks on the values its evaluator
//!   computes, such as division by zero. Array lengths, base slots and the zero divisor checks give
//!   up on constants nested more than 32 deep like solc; see TYPECK-020 in
//!   `docs/SOLC_DIVERGENCE.md`.
//! - [`Gcx::const_eval`] evaluates everything a constant initializer can contain, after type
//!   checking, with the declarations it selected, such as the function in `this.f.selector`.
//!   Codegen, base slots and other checks after type checking use it.
//!
//! The full mode computes the same value as the early mode wherever the early mode computes one.
//! Both are memoized queries: every expression and constant is evaluated at most once in each
//! mode, so constants that share other constants as a DAG take linear time, and repeated queries
//! are free.
//!
//! Codegen uses the value of a constant at every use. A constant without a value, such as
//! `keccak256(type(C).creationCode)` or one whose initializer reverts, gets a helper function that
//! runs its initializer at runtime instead; see CODEGEN-013 in `docs/SOLC_DIVERGENCE.md`.
//!
//! A constant's value is computed after the values of the constants it names: the query first
//! walks the reference graph of the constants it has not evaluated yet with an explicit stack,
//! then evaluates them in dependency order. Long chains of constants therefore do not recurse.
//! The constants of a reference cycle, found as strongly connected components of that graph, all
//! evaluate to [`EvalErrorKind::Cycle`]. The cycle check after type checking reports the cycles
//! themselves. Neither mode evaluates a constant through a type that is not computed yet, so the
//! graph has every edge: the early mode reaches no types, and the full mode reads types that type
//! checking computed.
//!
//! # Values
//!
//! A [`ConstValue`] carries its type: integers with their integer type, number literals, which
//! are exact rationals like solc's, booleans, `bytesN`, addresses and contracts, string literals,
//! `bytes` and `string` values, enum variants, user-defined value types, structs and arrays in
//! memory, and the bytecode of contracts, which only codegen knows. Operations follow the
//! language: typed arithmetic is checked against its type, literal arithmetic is exact with 4096
//! bits in the numerator and denominator, so `7 / 2 * 2` is 7, and conversions follow Solidity's
//! implicit and explicit conversion rules. Fractions only exist as literals: they convert to no
//! type, so they never reach codegen.
//!
//! # What folds
//!
//! The full mode folds literals, constants and their member accesses (`L.C`, `M.C`), unary and
//! binary operators, conditionals, `&&` and `||`, type conversions, `payable`, `type(T).min`,
//! `type(T).max`, `type(C).name`, `type(I).interfaceId`, enum variants, `.selector` of functions,
//! errors and events, `.length` and indexing of byte values, `keccak256`, `sha256`, `ripemd160`,
//! `addmod`, `mulmod`, `erc7201`, the `abi.encode` family, `bytes.concat`, `string.concat`,
//! `wrap` and `unwrap` of user-defined value types, struct constructors, `new` of byte arrays and
//! dynamic arrays, which hold zero values, and `type(C).creationCode` and `type(C).runtimeCode`,
//! whose bytes only codegen knows.
//!
//! An operation that reverts at runtime, such as an overflow, a division by zero or an
//! out-of-range conversion, does not fold. Neither does anything that reads state or the
//! environment, calls a function, builds array literals, or uses the bytes of contract code, nor
//! a value larger than 64 KiB. Every operand must evaluate, even the branch a conditional does not
//! take and the second operand of `&&` and `||`, so whether an expression folds does not depend
//! on which branch is taken. A constant without a value runs its initializer at runtime, which
//! skips the operands it does not need.

use crate::{
    hir,
    ty::{FxOnceMap, Gcx},
};
use EvalErrorKind as EE;
use alloy_primitives::U256;
use solar_data_structures::{
    graph::sccs,
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
    newtype_index,
};
use solar_interface::{
    Span,
    diagnostics::{DiagId, ErrorGuaranteed},
    error_code,
};
use std::{fmt, ops::ControlFlow};

mod builtins;
mod expr;
mod value;

pub use value::{BytesKind, CodeKind, ConstValue, FixedBytes, IntScalar};

/// Where a constant value is required, which selects solc's codes for evaluation errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstContext {
    /// The length of a fixed-size array type.
    ArrayLength,
    /// The base slot of a storage layout specifier.
    BaseSlot,
}

impl ConstContext {
    /// Returns the code solc reports when its evaluator gives the expression no value.
    pub fn unsupported_code(self) -> DiagId {
        match self {
            Self::ArrayLength => error_code!(5462),
            Self::BaseSlot => error_code!(1505),
        }
    }

    /// Returns the code of an error about a value: `code` if solc's evaluator computes the value
    /// too, and the code for unsupported expressions otherwise.
    pub fn value_code(self, solc: bool, code: DiagId) -> DiagId {
        if solc { code } else { self.unsupported_code() }
    }
}

/// How deep solc's constant evaluator follows constants before it gives up with error 5210.
const SOLC_MAX_DEPTH: u32 = 32;

/// What an evaluation computes; see the [module docs](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// What solc's `ConstantEvaluator` computes, from name resolution alone.
    Early,
    /// Everything a constant initializer can contain, with the results of type checking.
    Full,
}

/// The result of evaluating an expression or a constant.
pub(crate) type EvalResult = Result<ConstValue, EvalError>;

/// The memoized results of constant evaluation.
#[derive(Default)]
pub(crate) struct ConstEvalCache {
    early: ModeCache,
    full: ModeCache,
    /// The errors of literal operations that have no value, see
    /// [`ConstEval::emit_literal_op_error`].
    literal_op_errors: FxOnceMap<hir::ExprId, ErrorGuaranteed>,
    /// Checked array lengths, so each reports its errors once.
    array_lens: FxOnceMap<hir::ExprId, Result<U256, ErrorGuaranteed>>,
}

/// The memoized results of one evaluation mode.
#[derive(Default)]
struct ModeCache {
    /// Expression results. They own big-integer buffers, so keep them in the cache itself to drop
    /// them with the compilation instead of leaking them from the dropless HIR arena.
    exprs: FxOnceMap<hir::ExprId, Box<EvalResult>>,
    /// The values of constant variables, converted to their declared types when they fit.
    vars: FxOnceMap<hir::VariableId, Box<EvalResult>>,
}

impl<'gcx> Gcx<'gcx> {
    /// Returns the full constant evaluator, which needs the results of type checking.
    ///
    /// # Panics
    ///
    /// Panics before type checking has finished.
    pub fn const_eval(self) -> ConstEval<'gcx> {
        assert!(self.has_typeck_results(), "full constant evaluation before type checking");
        ConstEval { gcx: self, mode: Mode::Full }
    }

    /// Returns the evaluator of what solc's constant evaluator computes, which type checking can
    /// use.
    pub fn early_const_eval(self) -> ConstEval<'gcx> {
        ConstEval { gcx: self, mode: Mode::Early }
    }
}

/// The constant evaluator, with memoized queries for the values of expressions and constants.
///
/// See the [module docs](self) for the design, and [`Gcx::const_eval`] and
/// [`Gcx::early_const_eval`] to get one.
#[derive(Clone, Copy)]
pub struct ConstEval<'gcx> {
    gcx: Gcx<'gcx>,
    mode: Mode,
}

impl<'gcx> ConstEval<'gcx> {
    /// Evaluates the given expression to a typed constant value without emitting diagnostics.
    pub fn eval_expr(self, expr: &hir::Expr<'_>) -> Result<&'gcx ConstValue, EvalError> {
        self.expr_result(expr).as_ref().map_err(|err| *err)
    }

    /// Evaluates the given expression as an integer constant without emitting diagnostics.
    pub fn eval_int(self, expr: &hir::Expr<'_>) -> Result<&'gcx IntScalar, EvalError> {
        match self.eval_expr(expr)? {
            ConstValue::Int(value) => Ok(value),
            _ => Err(EE::NotAnInteger.spanned(expr.span)),
        }
    }

    /// Returns the value of the given constant variable, converted to its declared type when it
    /// fits.
    pub fn eval_var(self, id: hir::VariableId) -> Result<&'gcx ConstValue, EvalError> {
        self.var_result(id).as_ref().map_err(|err| *err)
    }

    /// Evaluates the given array size expression, emitting an error diagnostic if it fails.
    ///
    /// Like solc, this needs the early evaluator.
    ///
    /// Reference: <https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libsolidity/analysis/DeclarationTypeChecker.cpp#L336-L366>
    pub fn eval_array_len(self, size: &hir::Expr<'_>) -> Result<U256, ErrorGuaranteed> {
        debug_assert_eq!(self.mode, Mode::Early, "array lengths use the early evaluator");
        self.gcx.const_eval_cache.array_lens.insert_cloned(size.id, |_| self.check_array_len(size))
    }

    fn check_array_len(self, size: &hir::Expr<'_>) -> Result<U256, ErrorGuaranteed> {
        let cx = ConstContext::ArrayLength;
        let dcx = self.gcx.dcx();
        let int = match self.eval_with_depth_limit(size) {
            Ok(ConstValue::Int(int)) => int,
            Ok(ConstValue::Rational(_)) => {
                let msg = "array length cannot be fractional";
                return Err(dcx.err(msg).code(error_code!(3208)).span(size.span).emit());
            }
            Ok(_) => return Err(self.emit_error(size, EE::NotAnInteger.spanned(size.span), cx)),
            Err(err) => return Err(self.emit_error(size, err, cx)),
        };
        let Some(int) = int.as_u256() else {
            let err = if int.is_negative() {
                dcx.err("array length cannot be negative").code(error_code!(3658))
            } else {
                let err = dcx.err("array length is too large").code(error_code!(1847));
                err.note("the maximum is `2**256 - 1`")
            };
            return Err(err.span(size.span).emit());
        };
        if int.is_zero() {
            let msg = "array length must be greater than zero";
            Err(dcx.err(msg).code(error_code!(1406)).span(size.span).emit())
        } else {
            Ok(int)
        }
    }

    /// Evaluates an expression whose value solc requires, such as an array length, and returns
    /// [`EvalErrorKind::DepthLimit`] where solc's evaluator gives up on nested constants.
    pub(crate) fn eval_with_depth_limit(
        self,
        expr: &hir::Expr<'_>,
    ) -> Result<&'gcx ConstValue, EvalError> {
        debug_assert_eq!(self.mode, Mode::Early, "the depth limit is solc's");
        if let Some(id) = self.solc_depth_limit(expr) {
            return Err(EE::DepthLimit.spanned(self.gcx.hir.variable(id).span));
        }
        self.eval_expr(expr)
    }

    /// Like [`eval_with_depth_limit`](Self::eval_with_depth_limit), as an integer.
    pub(crate) fn eval_int_with_depth_limit(
        self,
        expr: &hir::Expr<'_>,
    ) -> Result<&'gcx IntScalar, EvalError> {
        match self.eval_with_depth_limit(expr)? {
            ConstValue::Int(value) => Ok(value),
            _ => Err(EE::NotAnInteger.spanned(expr.span)),
        }
    }

    /// Returns the constant at which solc's evaluator gives up on `expr` with error 5210.
    ///
    /// solc's evaluator follows the constants that identifiers name, at most 32 deep. It evaluates
    /// each constant once, the first time it reaches it, so the depth of a constant is that of the
    /// first path to it, in source order, whatever the constant's type.
    fn solc_depth_limit(self, expr: &hir::Expr<'_>) -> Option<hir::VariableId> {
        fn walk(
            cx: ConstEval<'_>,
            references: Vec<hir::VariableId>,
            depth: u32,
            seen: &mut FxHashSet<hir::VariableId>,
        ) -> ControlFlow<hir::VariableId> {
            for id in references {
                if seen.insert(id) {
                    if depth == SOLC_MAX_DEPTH {
                        return ControlFlow::Break(id);
                    }
                    walk(cx, cx.constant_references(id), depth + 1, seen)?;
                }
            }
            ControlFlow::Continue(())
        }

        walk(self, self.references_in(expr), 0, &mut FxHashSet::default()).break_value()
    }

    /// Returns the base slot of the storage of the given contract: the value of its storage layout
    /// specifier, or zero. The check after type checking reports invalid base slots.
    pub fn eval_base_slot(self, contract: hir::ContractId) -> U256 {
        let layout = self.gcx.hir.contract(contract).layout;
        layout.and_then(|layout| self.eval_int(layout).ok()?.as_u256()).unwrap_or_default()
    }

    pub(crate) fn expr_result(self, expr: &hir::Expr<'_>) -> &'gcx EvalResult {
        let cache = &self.cache().exprs;
        if let Some(result) = cache.get(&expr.id) {
            return result;
        }
        // The full mode has the value that the early mode computed.
        if self.mode == Mode::Full
            && let Some(result @ Ok(_)) = self.gcx.const_eval_cache.early.exprs.get(&expr.id)
        {
            return result;
        }
        // Evaluate before inserting: the evaluation inserts other expressions and constants.
        let result = expr::eval(self, expr);
        cache.insert(expr.id, |_| Box::new(result))
    }

    pub(crate) fn var_result(self, id: hir::VariableId) -> &'gcx EvalResult {
        let cache = &self.cache().vars;
        if let Some(result) = cache.get(&id) {
            return result;
        }
        if self.mode == Mode::Full
            && let Some(result @ Ok(_)) = self.gcx.const_eval_cache.early.vars.get(&id)
        {
            return result;
        }
        let references = self.constant_references(id);
        if references.iter().all(|&dep| dep != id && self.is_evaluated(dep)) {
            let result = self.eval_constant(id);
            return cache.insert(id, |_| Box::new(result));
        }
        self.eval_constants_in_order(id, references);
        cache.get(&id).expect("constant evaluated")
    }

    fn cache(self) -> &'gcx ModeCache {
        let cache = &self.gcx.const_eval_cache;
        match self.mode {
            Mode::Early => &cache.early,
            Mode::Full => &cache.full,
        }
    }

    /// Evaluates `root` and the constants it depends on that are not evaluated yet, each after the
    /// constants it names.
    fn eval_constants_in_order(self, root: hir::VariableId, root_references: Vec<hir::VariableId>) {
        newtype_index! {
            struct Node;
        }

        let cache = &self.cache().vars;
        let mut nodes = IndexVec::<Node, hir::VariableId>::new();
        let mut edges = IndexVec::<Node, Vec<Node>>::new();
        let mut index = FxHashMap::default();
        let mut stack = vec![root];
        index.insert(root, nodes.push(root));
        edges.push(Vec::new());
        let mut root_references = Some(root_references);
        while let Some(id) = stack.pop() {
            let node = index[&id];
            let references = root_references.take().unwrap_or_else(|| self.constant_references(id));
            for dep in references {
                if self.is_evaluated(dep) {
                    continue;
                }
                let dep = *index.entry(dep).or_insert_with(|| {
                    stack.push(dep);
                    edges.push(Vec::new());
                    nodes.push(dep)
                });
                edges[node].push(dep);
            }
        }

        // Components come in reverse topological order, so dependencies come first.
        for scc in sccs(nodes.len(), |node: Node| edges[node].iter().copied()) {
            if let [node] = scc[..]
                && !edges[node].contains(&node)
            {
                let id = nodes[node];
                let result = self.eval_constant(id);
                cache.insert(id, |_| Box::new(result));
            } else {
                for &node in &scc {
                    let id = nodes[node];
                    let err = EE::Cycle.spanned(self.gcx.hir.variable(id).span);
                    cache.insert(id, |_| Box::new(Err(err)));
                }
            }
        }
    }

    /// Returns whether the constant `id` is evaluated and not on a cycle.
    ///
    /// Another thread may have evaluated part of a cycle, so the cycle's constants are walked again
    /// to find the whole cycle.
    fn is_evaluated(self, id: hir::VariableId) -> bool {
        self.cache()
            .vars
            .get(&id)
            .is_some_and(|result| !matches!(result, Err(EvalError { kind: EE::Cycle, .. })))
    }

    /// Returns the constants that the initializer of the constant `id` names.
    fn constant_references(self, id: hir::VariableId) -> Vec<hir::VariableId> {
        self.gcx
            .hir
            .variable(id)
            .initializer
            .map(|init| self.references_in(init))
            .unwrap_or_default()
    }

    /// Returns the constants that the given expression names, in source order.
    fn references_in(self, expr: &hir::Expr<'_>) -> Vec<hir::VariableId> {
        let mut references = Vec::new();
        let _ = expr.visit(&mut |expr| {
            if let Some(dep) = self.constant_reference(expr) {
                references.push(dep);
            }
            ControlFlow::<()>::Continue(())
        });
        references
    }

    /// Returns the constant variable that the given expression names.
    fn constant_reference(self, expr: &hir::Expr<'_>) -> Option<hir::VariableId> {
        self.res(expr)?.as_variable().filter(|&id| self.gcx.hir.variable(id).is_constant())
    }

    /// Returns the declaration that the given path names: in the early mode only identifiers,
    /// which solc's evaluator follows, by name resolution, and in the full mode identifiers and
    /// member accesses of paths, such as `this.f`, by the declaration type checking selected.
    ///
    /// A member access of another expression, such as `h().f`, names nothing: evaluating it would
    /// drop the effects of its base.
    fn res(self, expr: &hir::Expr<'_>) -> Option<hir::Res> {
        fn is_path(expr: &hir::Expr<'_>) -> bool {
            match expr.peel_parens().kind {
                hir::ExprKind::Ident(_) => true,
                hir::ExprKind::Member(base, _) => is_path(base),
                _ => false,
            }
        }

        match (self.mode, &expr.kind) {
            (Mode::Early, hir::ExprKind::Ident(_)) => self.gcx.resolve_path_expr(expr),
            (Mode::Full, _) if is_path(expr) => {
                self.gcx.resolved_expr(expr).or_else(|| self.gcx.resolve_path_expr(expr))
            }
            _ => None,
        }
    }

    /// Evaluates the initializer of the constant `id` and converts it to the declared type.
    fn eval_constant(self, id: hir::VariableId) -> EvalResult {
        expr::eval_constant(self, id).map_err(|err| err.in_constant(id))
    }

    /// Emits a diagnostic for the given constant evaluation error of an expression whose value is
    /// required in `cx`.
    pub fn emit_error(
        self,
        expr: &hir::Expr<'_>,
        err: EvalError,
        cx: ConstContext,
    ) -> ErrorGuaranteed {
        // A failed literal operation in the initializer of a constant reads like it does
        // anywhere else.
        if err.kind.is_undefined_literal_op()
            && let Some(id) = err.constant
            && let Some(init) = self.gcx.hir.variable(id).initializer
            && let ControlFlow::Break(guar) = init.visit(&mut |op| match op.kind {
                hir::ExprKind::Binary(..) if op.span == err.span => {
                    ControlFlow::Break(self.emit_literal_op_error(op, err.kind))
                }
                _ => ControlFlow::Continue(()),
            })
        {
            return guar;
        }
        let code = match err.kind {
            EE::AlreadyEmitted(guar) => return guar,
            EE::LiteralTooLarge => return self.gcx.literal_too_large(err.span),
            // solc's evaluator gives the expression no value.
            _ if self.mode == Mode::Full => cx.unsupported_code(),
            EE::Cycle | EE::DepthLimit => error_code!(5210),
            EE::ArithmeticOverflow => error_code!(2643),
            EE::UnaryOverflow => error_code!(3667),
            // solc gives these literal operations no type.
            kind if kind.is_undefined_literal_op() => error_code!(6020),
            _ => cx.unsupported_code(),
        };
        let msg = format!("failed to evaluate constant: {}", err.kind.msg());
        let mut diag = self
            .gcx
            .dcx()
            .err(msg)
            .code(code)
            .span(expr.span)
            .span_label(err.span, "evaluation of constant value failed here");
        // Tell the user what solc's evaluator accepts in an array length.
        if cx == ConstContext::ArrayLength
            && matches!(err.kind, EE::UnsupportedExpr | EE::NonConstantVar | EE::NotAnInteger)
        {
            diag = diag.help(
                "array lengths take number literals, integer constants and arithmetic on them",
            );
        }
        diag.emit()
    }

    /// Reports the literal binary operation `expr`, which has no value, such as `1 / 0`.
    ///
    /// Type checking reports the operation, and so does the evaluation of an array length or base
    /// slot that reads a constant containing it. Either reports it once, in the same way.
    pub(crate) fn emit_literal_op_error(
        self,
        expr: &hir::Expr<'_>,
        kind: EvalErrorKind,
    ) -> ErrorGuaranteed {
        let hir::ExprKind::Binary(lhs, op, rhs) = expr.kind else {
            unreachable!("not a binary operation: {expr:?}")
        };
        self.gcx.const_eval_cache.literal_op_errors.insert_cloned(expr.id, |_| {
            let operand = |expr: &hir::Expr<'_>| {
                let ty = self.eval_expr(expr).ok().and_then(|value| value.literal_ty(self.gcx));
                (expr.span, ty.expect("literal operation operands have values"))
            };
            self.gcx.binop_err(op, operand(lhs), operand(rhs), false).note(kind.msg()).emit()
        })
    }
}

/// The kind of a constant evaluation error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvalErrorKind {
    /// The constant depends on itself.
    Cycle,
    /// The value reads constants more than 32 deep, where solc's evaluator gives up.
    DepthLimit,
    /// A binary operation leaves its type.
    ArithmeticOverflow,
    /// A unary operation leaves its type.
    UnaryOverflow,
    /// A literal operand of a typed operation has no integer type.
    LiteralTooLarge,
    /// Negating a value of an unsigned type.
    NegateUnsigned,
    /// A division or modulo by zero, of literals or of typed values, or a zero `addmod` or
    /// `mulmod` modulus. solc's evaluator reports only the literal one.
    DivisionByZero { literal: bool },
    /// A literal shift amount that solc gives no value.
    ShiftTooLarge,
    /// A negative literal shift amount.
    NegativeShift,
    /// A literal exponent that solc gives no value.
    ExponentTooLarge,
    /// A fraction where a value of a type is required.
    Fractional,
    /// Literal arithmetic beyond the 4096 bits of solc's rational numbers.
    LiteralPrecision,
    /// A conversion or index outside the range of its target.
    OutOfRange,
    /// The value is not an integer.
    NotAnInteger,
    /// A value too large to fold.
    ValueTooLarge,
    /// A unary operator the operand does not support.
    UnsupportedUnaryOp,
    /// A binary operator the operands do not support.
    UnsupportedBinaryOp,
    /// An expression the evaluator does not compute.
    UnsupportedExpr,
    /// A read of a variable that is not a constant.
    NonConstantVar,
    /// An error that was already reported, such as a type error in the expression.
    AlreadyEmitted(ErrorGuaranteed),
}

impl EvalErrorKind {
    pub fn spanned(self, span: Span) -> EvalError {
        EvalError { kind: self, span, constant: None }
    }

    pub fn msg(&self) -> &'static str {
        match self {
            Self::Cycle => "cyclic constant definition",
            Self::DepthLimit => "constants are nested too deeply",
            Self::ArithmeticOverflow | Self::UnaryOverflow => "arithmetic overflow",
            Self::LiteralTooLarge => "literal is too large for any integer type",
            Self::NegateUnsigned => "cannot apply unary operator `-` to an unsigned type",
            Self::DivisionByZero { .. } => "attempted to divide by zero",
            Self::ShiftTooLarge => "shift amount is too large",
            Self::NegativeShift => "shift amount is negative",
            Self::LiteralPrecision => "the precision of literal arithmetic is limited to 4096 bits",
            Self::ExponentTooLarge => "exponent is too large",
            Self::Fractional => "the result is not an integer",
            Self::OutOfRange => "value out of range",
            Self::NotAnInteger => "expected an integer",
            Self::ValueTooLarge => "value is too large",
            Self::UnsupportedUnaryOp => "unsupported unary operation",
            Self::UnsupportedBinaryOp => "unsupported binary operation",
            Self::UnsupportedExpr => "unsupported expression",
            Self::NonConstantVar => "only constant variables are allowed",
            Self::AlreadyEmitted(_) => "an error was already reported",
        }
    }

    /// Returns `true` if solc's evaluator gives the expression no value and reports nothing,
    /// because runtime code computes it or reverts, as for `keccak256(type(C).creationCode)`, a
    /// byte array too large to fold or a typed division by zero, or because type checking
    /// reports it, as for `-x` of an unsigned `x`.
    pub fn is_unsupported(&self) -> bool {
        match self {
            Self::ValueTooLarge
            | Self::UnsupportedUnaryOp
            | Self::UnsupportedBinaryOp
            | Self::UnsupportedExpr
            | Self::NonConstantVar
            | Self::NegateUnsigned
            | Self::DivisionByZero { literal: false } => true,
            Self::Cycle
            | Self::DepthLimit
            | Self::ArithmeticOverflow
            | Self::UnaryOverflow
            | Self::LiteralTooLarge
            | Self::DivisionByZero { literal: true }
            | Self::ShiftTooLarge
            | Self::NegativeShift
            | Self::ExponentTooLarge
            | Self::Fractional
            | Self::LiteralPrecision
            | Self::OutOfRange
            | Self::NotAnInteger
            | Self::AlreadyEmitted(_) => false,
        }
    }

    /// Returns `true` if solc gives no value to a literal operation that fails with this error.
    pub(crate) fn is_undefined_literal_op(&self) -> bool {
        matches!(
            self,
            Self::DivisionByZero { literal: true }
                | Self::ShiftTooLarge
                | Self::NegativeShift
                | Self::ExponentTooLarge
                | Self::LiteralPrecision
        )
    }
}

/// A constant evaluation error.
#[derive(Clone, Copy, Debug)]
pub struct EvalError {
    pub span: Span,
    pub kind: EvalErrorKind,
    /// The constant whose initializer contains the failed expression, if any.
    pub constant: Option<hir::VariableId>,
}

impl EvalError {
    /// Records that the failed expression is in the initializer of the constant `id`, unless it is
    /// in a constant that one reads.
    pub(crate) fn in_constant(self, id: hir::VariableId) -> Self {
        Self { constant: self.constant.or(Some(id)), ..self }
    }
}

impl From<EE> for EvalError {
    fn from(value: EE) -> Self {
        value.spanned(Span::DUMMY)
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.msg().fmt(f)
    }
}

impl std::error::Error for EvalError {}
