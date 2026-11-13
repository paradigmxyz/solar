//! Source specifications, lowered as records that travel through the optimizer.
//!
//! A function's NatSpec `@custom:requires <expr>` and `@custom:ensures <expr>` tags, the clauses
//! `solar-verify` reads, state what the function needs of its inputs and promises of its result.
//! With recording on (`-Zcheck-analyses` or `-Zprove-analyses`, see `lower-assumes`), this module
//! lowers each clause to an `i1` computed by ordinary pure MIR instructions and records it:
//!
//! ```text
//! entry:  r = <requires clause>; assume ne spec_requires, r, 0
//! return: e = <ensures clause>; c = or (eq r, 0), e; assume ne spec_ensures, c, 0; ret ...
//! ```
//!
//! A `requires` clause is a premise, recorded where the function starts. An `ensures` clause is an
//! obligation, recorded before each return as implied by the conjunction of the premises, so the
//! record keeps the premises it depends on even after inlining moves it into a caller. Passes
//! preserve the records like the facts they record themselves, and `lower-assumes` drops the
//! premises, tests each obligation at run time, and with `-Zprove-analyses` writes it as a query
//! about the optimized program, so a proof covers every optimization that ran.
//!
//! Clauses are Solidity expressions over the function's parameters, as they were on entry, and
//! its named return values, as returned: `bool`, integer and address operands, literals,
//! parentheses, `!`, unary `-` and `~`, comparisons, `&&`, `||`, `+`, `-`, `*`, `/`, `%` and the
//! bitwise operators. Arithmetic is on 256-bit words modulo 2^256 with the EVM's division by zero,
//! signed when an operand has a signed type; `&&` and `||` evaluate both sides, which is equal
//! since no clause has effects. A clause that uses anything else is reported and skipped.
//!
//! Without recording, no clause is lowered, so specifications never change the code the compiler
//! emits. With it, they change it as much as other records do: the obligations become run-time
//! tests, and the values a record reads stay alive until `lower-assumes`.

use super::*;
use crate::mir::{FactSource, InstId};
use solar_ast as ast;
use solar_evmir::debug::DebugLocKind;
use solar_parse::{Lexer, Parser};

/// The clauses of one function and the values they read, while it is lowered.
pub(super) struct FunctionSpec {
    ensures: Vec<SpecExpr>,
    /// The parameters' values on entry.
    params: FxHashMap<VariableId, ValueId>,
    /// The conjunction of the `requires` clauses, or `None` when there are none.
    premise: Option<ValueId>,
}

/// A clause after name resolution.
enum SpecExpr {
    Param(VariableId, Operand),
    /// The return value at this position.
    Return(usize, Operand),
    Number(U256),
    Bool(bool),
    Unary(UnOpKind, Box<Self>),
    Binary(BinOpKind, Box<Self>, Box<Self>),
}

impl SpecExpr {
    /// Whether the expression is a bool rather than a word, or `None` when it mixes them where
    /// [`FunctionLowerer::spec_binary`] does not accept it.
    fn is_bool(&self) -> Option<bool> {
        match self {
            Self::Param(_, operand) | Self::Return(_, operand) => {
                Some(matches!(operand, Operand::Bool))
            }
            Self::Number(_) => Some(false),
            Self::Bool(_) => Some(true),
            Self::Unary(op, operand) => {
                let is_bool = operand.is_bool()?;
                (is_bool == (*op == UnOpKind::Not)).then_some(is_bool)
            }
            Self::Binary(op, lhs, rhs) => {
                let is_bool = lhs.is_bool()?;
                if rhs.is_bool()? != is_bool {
                    return None;
                }
                match op {
                    BinOpKind::Eq | BinOpKind::Ne => Some(true),
                    BinOpKind::And | BinOpKind::Or => is_bool.then_some(true),
                    BinOpKind::Lt | BinOpKind::Le | BinOpKind::Gt | BinOpKind::Ge => {
                        (!is_bool).then_some(true)
                    }
                    BinOpKind::Add
                    | BinOpKind::Sub
                    | BinOpKind::Mul
                    | BinOpKind::Div
                    | BinOpKind::Rem
                    | BinOpKind::BitAnd
                    | BinOpKind::BitOr
                    | BinOpKind::BitXor => (!is_bool).then_some(false),
                    _ => None,
                }
            }
        }
    }
}

/// The type of a variable a clause reads.
#[derive(Clone, Copy)]
enum Operand {
    Bool,
    Word { bits: u16, signed: bool },
}

/// A lowered subexpression.
#[derive(Clone, Copy)]
enum SpecValue {
    Bool(ValueId),
    Word { value: ValueId, signed: bool },
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    /// Reads `function`'s specification and records its `requires` clauses at the current point,
    /// the function's entry. Does nothing without recording or clauses.
    pub(super) fn lower_spec_requires(&mut self, function: &hir::Function<'gcx>) {
        if !crate::mir::should_check_analyses(self.cx.gcx.sess) {
            return;
        }
        let Some((requires, ensures)) = parse_spec(self.cx.gcx, function) else { return };
        let params = function
            .parameters
            .iter()
            .filter_map(|&param| Some((param, *self.values.get(&param)?)))
            .collect();
        let mut spec = FunctionSpec { ensures, params, premise: None };
        for clause in &requires {
            let Some(condition) = self.lower_spec_clause(&spec, clause, &[]) else { continue };
            self.record_spec(FactSource::SpecRequires, condition);
            spec.premise = Some(match spec.premise {
                // p = and p, r
                Some(premise) => self.spec_inst(InstKind::And(premise, condition), MirType::I1),
                None => condition,
            });
        }
        self.spec = Some(spec);
    }

    /// Records every `ensures` clause before a return of `returns`.
    pub(super) fn record_spec_ensures(&mut self, returns: &[ValueId]) {
        let Some(spec) = self.spec.take() else { return };
        for clause in &spec.ensures {
            let Some(mut condition) = self.lower_spec_clause(&spec, clause, returns) else {
                continue;
            };
            if let Some(premise) = spec.premise {
                // c = or (eq premise, 0), e
                let zero = self.builder.imm_bool(false);
                let unmet = self.spec_inst(InstKind::Eq(premise, zero), MirType::I1);
                condition = self.spec_inst(InstKind::Or(unmet, condition), MirType::I1);
            }
            self.record_spec(FactSource::SpecEnsures, condition);
        }
        self.spec = Some(spec);
    }

    fn record_spec(&mut self, source: FactSource, condition: ValueId) {
        let zero = self.builder.imm_bool(false);
        let block = self.builder.current_block();
        // assume ne source, condition, 0
        crate::mir::transform::lower_assumes::record_spec(
            self.builder.func_mut(),
            block,
            source,
            condition,
            zero,
        );
    }

    /// Lowers a clause to an `i1`, or reports why it cannot be.
    fn lower_spec_clause(
        &mut self,
        spec: &FunctionSpec,
        clause: &SpecExpr,
        returns: &[ValueId],
    ) -> Option<ValueId> {
        match self.lower_spec_expr(spec, clause, returns)? {
            SpecValue::Bool(value) => Some(value),
            SpecValue::Word { .. } => None,
        }
    }

    fn lower_spec_expr(
        &mut self,
        spec: &FunctionSpec,
        expr: &SpecExpr,
        returns: &[ValueId],
    ) -> Option<SpecValue> {
        Some(match *expr {
            SpecExpr::Param(id, operand) => self.spec_operand(*spec.params.get(&id)?, operand),
            SpecExpr::Return(index, operand) => self.spec_operand(*returns.get(index)?, operand),
            SpecExpr::Number(value) => {
                SpecValue::Word { value: self.builder.imm(value), signed: false }
            }
            SpecExpr::Bool(value) => SpecValue::Bool(self.builder.imm_bool(value)),
            SpecExpr::Unary(op, ref operand) => {
                let operand = self.lower_spec_expr(spec, operand, returns)?;
                match (op, operand) {
                    // !b = eq b, 0
                    (UnOpKind::Not, SpecValue::Bool(value)) => {
                        let zero = self.builder.imm_bool(false);
                        SpecValue::Bool(self.spec_inst(InstKind::Eq(value, zero), MirType::I1))
                    }
                    // -w = sub 0, w
                    (UnOpKind::Neg, SpecValue::Word { value, .. }) => {
                        let zero = self.builder.imm(U256::ZERO);
                        let value = self.spec_inst(InstKind::Sub(zero, value), MirType::I256);
                        SpecValue::Word { value, signed: true }
                    }
                    // ~w = not w
                    (UnOpKind::BitNot, SpecValue::Word { value, signed }) => {
                        let value = self.spec_inst(InstKind::Not(value), MirType::I256);
                        SpecValue::Word { value, signed }
                    }
                    _ => return None,
                }
            }
            SpecExpr::Binary(op, ref lhs, ref rhs) => {
                let lhs = self.lower_spec_expr(spec, lhs, returns)?;
                let rhs = self.lower_spec_expr(spec, rhs, returns)?;
                self.spec_binary(op, lhs, rhs)?
            }
        })
    }

    fn spec_binary(&mut self, op: BinOpKind, lhs: SpecValue, rhs: SpecValue) -> Option<SpecValue> {
        let bool = |this: &mut Self, kind| SpecValue::Bool(this.spec_inst(kind, MirType::I1));
        Some(match (lhs, rhs) {
            (SpecValue::Bool(a), SpecValue::Bool(b)) => match op {
                BinOpKind::And => bool(self, InstKind::And(a, b)),
                BinOpKind::Or => bool(self, InstKind::Or(a, b)),
                BinOpKind::Eq => bool(self, InstKind::Eq(a, b)),
                BinOpKind::Ne => bool(self, InstKind::Ne(a, b)),
                _ => return None,
            },
            (
                SpecValue::Word { value: a, signed: a_signed },
                SpecValue::Word { value: b, signed: b_signed },
            ) => {
                let signed = a_signed || b_signed;
                let word = |this: &mut Self, kind| SpecValue::Word {
                    value: this.spec_inst(kind, MirType::I256),
                    signed,
                };
                let lt = |a, b| if signed { InstKind::SLt(a, b) } else { InstKind::Lt(a, b) };
                let gt = |a, b| if signed { InstKind::SGt(a, b) } else { InstKind::Gt(a, b) };
                match op {
                    BinOpKind::Lt => bool(self, lt(a, b)),
                    BinOpKind::Gt => bool(self, gt(a, b)),
                    // a <= b = eq (gt a, b), 0
                    BinOpKind::Le => {
                        let greater = self.spec_inst(gt(a, b), MirType::I1);
                        let zero = self.builder.imm_bool(false);
                        bool(self, InstKind::Eq(greater, zero))
                    }
                    // a >= b = eq (lt a, b), 0
                    BinOpKind::Ge => {
                        let less = self.spec_inst(lt(a, b), MirType::I1);
                        let zero = self.builder.imm_bool(false);
                        bool(self, InstKind::Eq(less, zero))
                    }
                    BinOpKind::Eq => bool(self, InstKind::Eq(a, b)),
                    BinOpKind::Ne => bool(self, InstKind::Ne(a, b)),
                    BinOpKind::Add => word(self, InstKind::Add(a, b)),
                    BinOpKind::Sub => word(self, InstKind::Sub(a, b)),
                    BinOpKind::Mul => word(self, InstKind::Mul(a, b)),
                    BinOpKind::Div if signed => word(self, InstKind::SDiv(a, b)),
                    BinOpKind::Div => word(self, InstKind::Div(a, b)),
                    BinOpKind::Rem if signed => word(self, InstKind::SMod(a, b)),
                    BinOpKind::Rem => word(self, InstKind::Mod(a, b)),
                    BinOpKind::BitAnd => word(self, InstKind::And(a, b)),
                    BinOpKind::BitOr => word(self, InstKind::Or(a, b)),
                    BinOpKind::BitXor => word(self, InstKind::Xor(a, b)),
                    _ => return None,
                }
            }
            _ => return None,
        })
    }

    /// Reads a variable's value as its source type: a bool as an `i1`, an integer or address as a
    /// clean 256-bit word, sign-extended when signed.
    fn spec_operand(&mut self, value: ValueId, operand: Operand) -> SpecValue {
        match operand {
            Operand::Bool if self.builder.func().value_ty(value) == Some(MirType::I1) => {
                SpecValue::Bool(value)
            }
            // b = ne raw, 0
            Operand::Bool => {
                let value = self.spec_cast(value);
                let zero = self.builder.imm(U256::ZERO);
                SpecValue::Bool(self.spec_inst(InstKind::Ne(value, zero), MirType::I1))
            }
            Operand::Word { bits, signed } => {
                let mut value = self.spec_cast(value);
                if bits < 256 {
                    value = if signed {
                        // w = signextend bits / 8 - 1, raw
                        let byte = self.builder.imm(U256::from(bits / 8 - 1));
                        self.spec_inst(InstKind::SignExtend(byte, value), MirType::I256)
                    } else {
                        // w = and raw, 2^bits - 1
                        let mask = self.builder.imm((U256::from(1) << bits) - U256::from(1));
                        self.spec_inst(InstKind::And(value, mask), MirType::I256)
                    };
                }
                SpecValue::Word { value, signed }
            }
        }
    }

    /// Widens `value` to a word, marking the cast as specification code.
    fn spec_cast(&mut self, value: ValueId) -> ValueId {
        let first = self.builder.func().num_insts();
        // w = zext value to i256
        let value = self.builder.cast(value, MirType::I256);
        self.drop_spec_locations(first);
        value
    }

    /// Emits a pure instruction of a clause. Its location is dropped: no source statement
    /// computes it.
    fn spec_inst(&mut self, kind: InstKind, ty: MirType) -> ValueId {
        let first = self.builder.func().num_insts();
        let value = self.builder.emit_inst(kind, Some(ty));
        self.drop_spec_locations(first);
        value
    }

    fn drop_spec_locations(&mut self, first: usize) {
        let func = self.builder.func_mut();
        for index in first..func.num_insts() {
            func.inst_mut(InstId::from_usize(index)).metadata.drop_debug_loc(DebugLocKind::Dropped);
        }
    }
}

/// Parses and resolves `function`'s `@custom:requires` and `@custom:ensures` clauses, returning
/// `None` when it has none. A clause that does not parse or names anything but a parameter or a
/// named return value is reported as a warning and skipped.
fn parse_spec(
    gcx: Gcx<'_>,
    function: &hir::Function<'_>,
) -> Option<(Vec<SpecExpr>, Vec<SpecExpr>)> {
    let docs = gcx.hir.doc(function.doc).ast_comments();
    let mut requires = Vec::new();
    let mut ensures = Vec::new();
    for item in docs.iter().flat_map(|doc| doc.natspec.iter()) {
        let ast::NatSpecKind::Custom { name } = item.kind else { continue };
        let clauses = match name.name {
            sym::requires => &mut requires,
            sym::ensures => &mut ensures,
            _ => continue,
        };
        let content = solar_sema::solar_tag_content(docs, item.span);
        // NOTE: the clause's spans start one byte after its tag, which is exact for a clause on
        // the tag's line after one space and only places diagnostics otherwise.
        let start = solar_interface::BytePos(item.span.hi().0 + 1);
        let tokens = Lexer::with_start_pos(gcx.sess, &content, start).into_tokens();
        let arena = ast::Arena::new();
        let mut parser = Parser::new(gcx.sess, &arena, tokens);
        let expr = match parser.parse_expr() {
            Ok(expr) if parser.token.is_eof() => expr,
            Ok(_) => {
                warn_unsupported(gcx, parser.token.span, "unexpected token after the clause");
                continue;
            }
            Err(diag) => {
                diag.cancel();
                warn_unsupported(gcx, item.span, "this clause does not parse");
                continue;
            }
        };
        match resolve(gcx, function, expr) {
            Ok(clause) if clause.is_bool() == Some(true) => clauses.push(clause),
            Ok(_) => warn_unsupported(gcx, expr.span, "not a well-typed boolean clause"),
            Err(span) => warn_unsupported(gcx, span, "unsupported in a specification"),
        }
    }
    (!requires.is_empty() || !ensures.is_empty()).then_some((requires, ensures))
}

fn warn_unsupported(gcx: Gcx<'_>, span: Span, note: &str) {
    gcx.dcx().warn("specification clause skipped").span(span).note(note.to_string()).emit();
}

/// Resolves a parsed clause, or returns the span of what it cannot express.
fn resolve(
    gcx: Gcx<'_>,
    function: &hir::Function<'_>,
    expr: &ast::Expr<'_>,
) -> Result<SpecExpr, Span> {
    let span = expr.span;
    Ok(match &expr.kind {
        ast::ExprKind::Ident(ident) => {
            let named = |id: &VariableId| {
                gcx.hir.variable(*id).name.is_some_and(|name| name.name == ident.name)
            };
            if let Some(&id) = function.parameters.iter().rev().find(|id| named(id)) {
                SpecExpr::Param(id, operand(gcx, id).ok_or(span)?)
            } else if let Some(index) = function.returns.iter().position(named) {
                SpecExpr::Return(index, operand(gcx, function.returns[index]).ok_or(span)?)
            } else {
                return Err(span);
            }
        }
        ast::ExprKind::Lit(lit, _) => match lit.kind {
            ast::LitKind::Number(value) => SpecExpr::Number(value),
            ast::LitKind::Bool(value) => SpecExpr::Bool(value),
            _ => return Err(span),
        },
        ast::ExprKind::Tuple(elements) if elements.len() == 1 => {
            let Some(inner) = elements[0].as_ref().unspan() else { return Err(span) };
            resolve(gcx, function, inner)?
        }
        ast::ExprKind::Unary(op, operand)
            if matches!(op.kind, UnOpKind::Not | UnOpKind::Neg | UnOpKind::BitNot) =>
        {
            SpecExpr::Unary(op.kind, Box::new(resolve(gcx, function, operand)?))
        }
        ast::ExprKind::Binary(lhs, op, rhs) => SpecExpr::Binary(
            op.kind,
            Box::new(resolve(gcx, function, lhs)?),
            Box::new(resolve(gcx, function, rhs)?),
        ),
        _ => return Err(span),
    })
}

/// The type of a variable a clause can read: a bool, an integer or an address.
fn operand(gcx: Gcx<'_>, id: VariableId) -> Option<Operand> {
    let TyKind::Elementary(elementary) = gcx.type_of_item(id.into()).kind else { return None };
    Some(match elementary {
        ElementaryType::Bool => Operand::Bool,
        ElementaryType::UInt(size) => Operand::Word { bits: size.bits(), signed: false },
        ElementaryType::Int(size) => Operand::Word { bits: size.bits(), signed: true },
        ElementaryType::Address(_) => Operand::Word { bits: 160, signed: false },
        _ => return None,
    })
}
