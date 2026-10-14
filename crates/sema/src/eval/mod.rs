//! Constant evaluation.
//!
//! This evaluates side-effect free HIR expressions to typed values, like rustc's const eval does
//! for `const` items, but without an interpreter: it folds the expressions and builtins a constant
//! initializer can contain and never calls user code.
//!
//! # Queries
//!
//! [`Gcx::const_eval`] returns the [`ConstEval`] evaluator, whose
//! [`eval_expr`](ConstEval::eval_expr) evaluates an expression and
//! [`eval_var`](ConstEval::eval_var) the value of a constant variable, converted to its declared
//! type when it fits. Both are memoized queries: every expression and constant is evaluated at most
//! once, so constants that share other constants as a DAG take linear time, and repeated queries
//! from type checking and codegen are free. Values only depend on the HIR and name resolution,
//! never on type checking results, so a value is the same whenever it is first requested.
//!
//! Codegen uses the value of a constant at every use, through [`ConstEval::require_var`], which
//! reports once a constant whose initializer reverts (see CODEGEN-013 in
//! `docs/SOLC_DIVERGENCE.md`). A constant the evaluator does not compute
//! ([`EvalErrorKind::is_unsupported`]) runs its initializer at every use instead.
//!
//! A constant's value is computed after the values of the constants it names: the query first
//! walks the reference graph of the constants it has not evaluated yet with an explicit stack,
//! then evaluates them in dependency order. Long chains of constants therefore do not recurse.
//! The constants of a reference cycle, found as strongly connected components of that graph, all
//! evaluate to [`EvalErrorKind::Cycle`]. The cycle check after type checking reports the cycles
//! themselves.
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
//! Literals, constants and their member accesses (`L.C`, `M.C`), unary and binary operators,
//! conditionals, `&&` and `||`, type conversions, `payable`, `type(T).min`,
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
//! a selector whose signature depends on a constant, nor a value larger than 64 KiB. Every operand
//! must evaluate, even the branch a conditional does not take and the second operand of `&&` and
//! `||`: the branches decide the type of a conditional, and a constant with an operand that
//! reverts is a mistake.
//!
//! # solc's evaluator
//!
//! solc's `ConstantEvaluator` folds much less: number and string literals, constants of integer
//! and string types, arithmetic, and `erc7201`. Some of its checks only apply to values it
//! computes, such as the division by zero error, so every value records whether solc computes it
//! too; see [`ConstEval::eval_solc`]. solc only follows constants through identifiers, and at
//! most 32 deep. Likewise, every error records whether solc reports it or gives the expression no
//! value instead, which decides the code of the error that [`ConstEval::emit_error`] reports.

use crate::{
    hir,
    ty::{FxOnceMap, Gcx},
};
use EvalErrorKind as EE;
use alloy_primitives::U256;
use solar_data_structures::{graph::sccs, index::IndexVec, map::FxHashMap, newtype_index};
use solar_interface::{
    Span,
    diagnostics::{DiagId, ErrorGuaranteed},
    error_code,
};
use std::{cell::RefCell, fmt, ops::ControlFlow};

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

/// A value and whether solc's constant evaluator computes it too.
#[derive(Clone, Debug)]
pub(crate) struct Folded {
    value: ConstValue,
    /// The number of nested constants solc's evaluator reads to compute the value, or `None` if
    /// it does not compute it.
    solc: Option<u32>,
}

impl Folded {
    fn new(value: ConstValue) -> Self {
        Self { value, solc: None }
    }

    fn solc(value: ConstValue, solc: Option<u32>) -> Self {
        Self { value, solc: solc.filter(|&depth| depth <= SOLC_MAX_DEPTH) }
    }
}

/// The result of evaluating an expression or a constant.
pub(crate) type EvalResult = Result<Folded, EvalError>;

/// The memoized results of constant evaluation.
#[derive(Default)]
pub(crate) struct ConstEvalCache {
    /// Expression results. They own big-integer buffers, so keep them in the cache itself to drop
    /// them with the compilation instead of leaking them from the dropless HIR arena.
    exprs: FxOnceMap<hir::ExprId, Box<EvalResult>>,
    /// The values of constant variables, converted to their declared types when they fit.
    vars: FxOnceMap<hir::VariableId, Box<EvalResult>>,
    /// The errors of literal operations that have no value, see
    /// [`ConstEval::emit_literal_op_error`].
    literal_op_errors: FxOnceMap<hir::ExprId, ErrorGuaranteed>,
    /// The errors of constants whose values are required, see [`ConstEval::require_var`].
    var_errors: FxOnceMap<hir::VariableId, ErrorGuaranteed>,
}

thread_local! {
    /// The constants whose initializers this thread is evaluating.
    ///
    /// Evaluation can reach a constant again through a type instead of a reference, such as the
    /// array length in `new uint[C][](1)`, whose zero value it builds. Such a constant depends on
    /// itself, which the reference graph does not show.
    static IN_PROGRESS: RefCell<Vec<hir::VariableId>> = const { RefCell::new(Vec::new()) };
}

/// Returns whether this thread is evaluating the initializer of the constant `id`.
fn in_progress(id: hir::VariableId) -> bool {
    IN_PROGRESS.with_borrow(|stack| stack.contains(&id))
}

impl<'gcx> Gcx<'gcx> {
    /// Returns the constant evaluator.
    pub fn const_eval(self) -> ConstEval<'gcx> {
        ConstEval { gcx: self }
    }
}

/// The constant evaluator, with memoized queries for the values of expressions and constants.
///
/// See the [module docs](self) for the design, and [`Gcx::const_eval`] to get one.
#[derive(Clone, Copy)]
pub struct ConstEval<'gcx> {
    gcx: Gcx<'gcx>,
}

impl<'gcx> ConstEval<'gcx> {
    /// Evaluates the given expression to a typed constant value without emitting diagnostics.
    pub fn eval_expr(self, expr: &hir::Expr<'_>) -> Result<&'gcx ConstValue, EvalError> {
        self.expr_result(expr).as_ref().map(|folded| &folded.value).map_err(|err| *err)
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
        self.var_result(id).as_ref().map(|folded| &folded.value).map_err(|err| *err)
    }

    /// Returns the value of the given constant variable where it is required, such as at its uses
    /// in codegen, reporting its evaluation error once.
    ///
    /// An error in a constant that this one reads is reported at that constant. Unlike solc, which
    /// evaluates the initializer at every use and can revert there, a constant whose initializer
    /// reverts is an error.
    pub fn require_var(self, id: hir::VariableId) -> Result<&'gcx ConstValue, ErrorGuaranteed> {
        self.eval_var(id).map_err(|err| {
            let root = err.constant.unwrap_or(id);
            self.gcx.const_eval_cache.var_errors.insert_cloned(root, |_| {
                let var = self.gcx.hir.variable(root);
                let span = var.initializer.map_or(var.span, |init| init.span);
                self.emit_error_at(span, err, None)
            })
        })
    }

    /// Returns the value of the given expression if solc's constant evaluator computes it too.
    ///
    /// Checks that solc runs on constant operands, such as division by zero, use this to report
    /// exactly what solc reports.
    pub fn eval_solc(self, expr: &hir::Expr<'_>) -> Option<&'gcx ConstValue> {
        let folded = self.expr_result(expr).as_ref().ok()?;
        folded.solc.map(|_| &folded.value)
    }

    /// Evaluates the given array size expression, emitting an error diagnostic if it fails.
    ///
    /// Reference: <https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libsolidity/analysis/DeclarationTypeChecker.cpp#L336-L366>
    pub fn eval_array_len(self, size: &hir::Expr<'_>) -> Result<U256, ErrorGuaranteed> {
        let cx = ConstContext::ArrayLength;
        let (int, solc) = match self.expr_result(size) {
            Ok(Folded { value: ConstValue::Int(int), solc }) => (int, solc.is_some()),
            Ok(Folded { value: ConstValue::Rational(_), solc }) => {
                let code = cx.value_code(solc.is_some(), error_code!(3208));
                let msg = "array length cannot be fractional";
                return Err(self.gcx.dcx().err(msg).code(code).span(size.span).emit());
            }
            Ok(_) => {
                let err = EE::NotAnInteger.spanned(size.span);
                return Err(self.emit_error(size, err, cx));
            }
            Err(err) => return Err(self.emit_error(size, *err, cx)),
        };
        let code = |code| cx.value_code(solc, code);
        let Some(int) = int.as_u256() else {
            let dcx = self.gcx.dcx();
            let err = if int.is_negative() {
                dcx.err("array length cannot be negative").code(code(error_code!(3658)))
            } else {
                let err = dcx.err("array length is too large").code(code(error_code!(1847)));
                err.note("the maximum is `2**256 - 1`")
            };
            return Err(err.span(size.span).emit());
        };
        if int.is_zero() {
            let msg = "array length must be greater than zero";
            Err(self.gcx.dcx().err(msg).code(code(error_code!(1406))).span(size.span).emit())
        } else {
            Ok(int)
        }
    }

    /// Returns the base slot of the storage of the given contract: the value of its storage layout
    /// specifier, or zero. Type checking reports invalid base slots.
    pub fn eval_base_slot(self, contract: hir::ContractId) -> U256 {
        let layout = self.gcx.hir.contract(contract).layout;
        layout.and_then(|layout| self.eval_int(layout).ok()?.as_u256()).unwrap_or_default()
    }

    pub(crate) fn expr_result(self, expr: &hir::Expr<'_>) -> &'gcx EvalResult {
        let cache = &self.gcx.const_eval_cache.exprs;
        if let Some(result) = cache.get(&expr.id) {
            return result;
        }
        // Evaluate before inserting: the evaluation inserts other expressions and constants.
        let result = expr::eval(self.gcx, expr);
        cache.insert(expr.id, |_| Box::new(result))
    }

    pub(crate) fn var_result(self, id: hir::VariableId) -> &'gcx EvalResult {
        let cache = &self.gcx.const_eval_cache.vars;
        if let Some(result) = cache.get(&id) {
            return result;
        }
        if in_progress(id) {
            let span = self.gcx.hir.variable(id).span;
            return self.gcx.cached_arena().alloc(Err(EE::Cycle.spanned(span)));
        }
        let references = self.constant_references(id);
        if references.iter().all(|&(dep, _)| dep != id && self.is_evaluated(dep)) {
            let result = self.eval_constant(id);
            return cache.insert(id, |_| Box::new(result));
        }
        self.eval_constants_in_order(id, references);
        cache.get(&id).expect("constant evaluated")
    }

    /// Evaluates `root` and the constants it depends on that are not evaluated yet, each after the
    /// constants it names.
    fn eval_constants_in_order(
        self,
        root: hir::VariableId,
        root_references: Vec<(hir::VariableId, bool)>,
    ) {
        newtype_index! {
            struct Node;
        }

        let cache = &self.gcx.const_eval_cache.vars;
        let mut nodes = IndexVec::<Node, hir::VariableId>::new();
        // The constants each one names, and whether it names them by identifier.
        let mut edges = IndexVec::<Node, Vec<(Node, bool)>>::new();
        let mut index = FxHashMap::default();
        let mut stack = vec![root];
        index.insert(root, nodes.push(root));
        edges.push(Vec::new());
        let mut root_references = Some(root_references);
        while let Some(id) = stack.pop() {
            let node = index[&id];
            let references = root_references.take().unwrap_or_else(|| self.constant_references(id));
            for (dep, ident) in references {
                // A constant being evaluated is reached again through a type, which `var_result`
                // reports as a cycle.
                if self.is_evaluated(dep) || in_progress(dep) {
                    continue;
                }
                let dep = *index.entry(dep).or_insert_with(|| {
                    stack.push(dep);
                    edges.push(Vec::new());
                    nodes.push(dep)
                });
                edges[node].push((dep, ident));
            }
        }

        // Components come in reverse topological order, so dependencies come first.
        for scc in sccs(nodes.len(), |node: Node| edges[node].iter().map(|&(dep, _)| dep)) {
            if let [node] = scc[..]
                && !edges[node].iter().any(|&(dep, _)| dep == node)
            {
                let id = nodes[node];
                let result = self.eval_constant(id);
                cache.insert(id, |_| Box::new(result));
            } else {
                // solc's evaluator follows identifiers only, so it reaches the cycle if the
                // constants name each other by identifier.
                let solc = scc.iter().all(|&node| {
                    edges[node].iter().all(|&(dep, ident)| ident || !scc.contains(&dep))
                });
                for &node in &scc {
                    let id = nodes[node];
                    let err = EE::Cycle.spanned(self.gcx.hir.variable(id).span).solc(solc);
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
        self.gcx
            .const_eval_cache
            .vars
            .get(&id)
            .is_some_and(|result| !matches!(result, Err(EvalError { kind: EE::Cycle, .. })))
    }

    /// Returns the constants that the initializer of the constant `id` names, and whether it names
    /// each by identifier rather than by member access.
    fn constant_references(self, id: hir::VariableId) -> Vec<(hir::VariableId, bool)> {
        let mut references = Vec::new();
        if let Some(init) = self.gcx.hir.variable(id).initializer {
            let _ = init.visit(&mut |expr| {
                if let Some(dep) = self.constant_reference(expr) {
                    references.push((dep, matches!(expr.kind, hir::ExprKind::Ident(_))));
                }
                ControlFlow::<()>::Continue(())
            });
        }
        references
    }

    /// Returns the constant variable that the given identifier or member access names.
    pub(crate) fn constant_reference(self, expr: &hir::Expr<'_>) -> Option<hir::VariableId> {
        self.gcx
            .resolve_path_expr(expr)?
            .as_variable()
            .filter(|&id| self.gcx.hir.variable(id).is_constant())
    }

    /// Evaluates the initializer of the constant `id` and converts it to the declared type.
    fn eval_constant(self, id: hir::VariableId) -> EvalResult {
        IN_PROGRESS.with_borrow_mut(|stack| stack.push(id));
        let result = expr::eval_constant(self.gcx, id).map_err(|err| err.in_constant(id));
        IN_PROGRESS.with_borrow_mut(|stack| stack.pop());
        result
    }

    /// Emits a diagnostic for the given constant evaluation error of an expression whose value is
    /// required in `cx`.
    pub fn emit_error(
        self,
        expr: &hir::Expr<'_>,
        err: EvalError,
        cx: ConstContext,
    ) -> ErrorGuaranteed {
        self.emit_error_at(expr.span, err, Some(cx))
    }

    /// Emits a diagnostic for the given constant evaluation error of the expression at `span`,
    /// whose value is required in `cx`, or at the uses of a constant without a context.
    fn emit_error_at(
        self,
        span: Span,
        err: EvalError,
        cx: Option<ConstContext>,
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
        let code = match (err.kind, cx) {
            (EE::AlreadyEmitted(guar), _) => return guar,
            (EE::LiteralTooLarge, _) => return self.gcx.literal_too_large(err.span),
            // solc evaluates the uses of constants at runtime.
            (_, None) => None,
            // solc gives the expression no value instead of reporting this error.
            (_, Some(cx)) if !err.solc => Some(cx.unsupported_code()),
            (EE::Cycle, _) => Some(error_code!(5210)),
            (EE::ArithmeticOverflow, _) => Some(error_code!(2643)),
            (EE::UnaryOverflow, _) => Some(error_code!(3667)),
            // solc gives these literal operations no type.
            (kind, _) if kind.is_undefined_literal_op() => Some(error_code!(6020)),
            (_, Some(cx)) => Some(cx.unsupported_code()),
        };
        let msg = format!("failed to evaluate constant: {}", err.kind.msg());
        let mut diag = self.gcx.dcx().err(msg).span(span);
        diag = diag.span_label(err.span, "evaluation of constant value failed here");
        if let Some(code) = code {
            diag = diag.code(code);
        }
        if cx.is_none() {
            diag = diag.note("constants must have a value at compile time");
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
    /// A binary operation leaves its type.
    ArithmeticOverflow,
    /// A unary operation leaves its type.
    UnaryOverflow,
    /// A literal operand of a typed operation has no integer type.
    LiteralTooLarge,
    /// Negating a value of an unsigned type.
    NegateUnsigned,
    /// A division or modulo by zero, of literals or of typed values, or a zero `addmod` or
    /// `mulmod` modulus.
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
        EvalError { kind: self, span, solc: false, constant: None }
    }

    pub fn msg(&self) -> &'static str {
        match self {
            Self::Cycle => "cyclic constant definition",
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

    /// Returns `true` if the evaluator does not compute a value that runtime code can compute,
    /// such as `keccak256(type(C).creationCode)` or a byte array too large to fold.
    pub fn is_unsupported(&self) -> bool {
        match self {
            Self::ValueTooLarge
            | Self::UnsupportedUnaryOp
            | Self::UnsupportedBinaryOp
            | Self::UnsupportedExpr => true,
            Self::Cycle
            | Self::ArithmeticOverflow
            | Self::UnaryOverflow
            | Self::LiteralTooLarge
            | Self::NegateUnsigned
            | Self::DivisionByZero { .. }
            | Self::ShiftTooLarge
            | Self::NegativeShift
            | Self::ExponentTooLarge
            | Self::Fractional
            | Self::LiteralPrecision
            | Self::OutOfRange
            | Self::NotAnInteger
            | Self::NonConstantVar
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
    /// Whether solc's constant evaluator reports this error too, rather than giving the
    /// expression no value.
    pub solc: bool,
    /// The constant whose initializer contains the failed expression, if any.
    pub constant: Option<hir::VariableId>,
}

impl EvalError {
    pub(crate) fn solc(self, solc: bool) -> Self {
        Self { solc, ..self }
    }

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
