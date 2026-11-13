//! The word semantics of pure EVM opcodes.
//!
//! [`eval_opcode`] computes what a pure opcode pushes for concrete operands, with the EVM's
//! wrapping arithmetic, zero divisors, and saturating shifts. Constant folding and the interpreters
//! share it, so every consumer has one definition of each opcode. [`smt_opcode`] gives the same
//! semantics as an SMT-LIB term, for the queries that prove analysis facts.

use crate::op;
use alloy_primitives::U256;
use std::cmp::Ordering;

type Word = U256;

/// Evaluates a pure EVM opcode with concrete operands in pop order.
pub fn eval_opcode(opcode: u8, operands: &[U256]) -> Option<U256> {
    Some(match (opcode, operands) {
        (op::ADD, &[a, b]) => a.wrapping_add(b),
        (op::SUB, &[a, b]) => a.wrapping_sub(b),
        (op::MUL, &[a, b]) => a.wrapping_mul(b),
        (op::DIV, &[a, b]) => div(a, b),
        (op::SDIV, &[a, b]) => i256_div(a, b),
        (op::MOD, &[a, b]) => rem(a, b),
        (op::SMOD, &[a, b]) => i256_mod(a, b),
        (op::EXP, &[a, b]) => a.wrapping_pow(b),
        (op::ADDMOD, &[a, b, n]) => a.add_mod(b, n),
        (op::MULMOD, &[a, b, n]) => a.mul_mod(b, n),
        (op::AND, &[a, b]) => a & b,
        (op::OR, &[a, b]) => a | b,
        (op::XOR, &[a, b]) => a ^ b,
        (op::NOT, &[a]) => !a,
        (op::CLZ, &[a]) => U256::from(a.leading_zeros()),
        (op::SHL, &[shift, value]) => shl(shift, value),
        (op::SHR, &[shift, value]) => shr(shift, value),
        (op::SAR, &[shift, value]) => sar(shift, value),
        (op::BYTE, &[index, value]) => byte(index, value),
        (op::SIGNEXTEND, &[size, value]) => signextend(size, value),
        (op::LT, &[a, b]) => U256::from(a < b),
        (op::GT, &[a, b]) => U256::from(a > b),
        (op::SLT, &[a, b]) => U256::from(i256_cmp(&a, &b) == Ordering::Less),
        (op::SGT, &[a, b]) => U256::from(i256_cmp(&a, &b) == Ordering::Greater),
        (op::EQ, &[a, b]) => U256::from(a == b),
        (op::ISZERO, &[a]) => U256::from(a.is_zero()),
        _ => return None,
    })
}

/// The SMT-LIB term of a pure EVM opcode over 256-bit bit-vector operands in pop order, with
/// the same semantics as [`eval_opcode`]: wrapping arithmetic, zero divisors and saturating
/// shifts. `None` for an opcode it does not model.
pub fn smt_opcode(opcode: u8, operands: &[&str]) -> Option<String> {
    let zero = smt_word(U256::ZERO);
    let one = smt_word(U256::ONE);
    let bool_word = |condition: String| format!("(ite {condition} {one} {zero})");
    let shift_limit = smt_word(U256::from(256));
    Some(match (opcode, operands) {
        (op::ADD, [a, b]) => format!("(bvadd {a} {b})"),
        (op::SUB, [a, b]) => format!("(bvsub {a} {b})"),
        (op::MUL, [a, b]) => format!("(bvmul {a} {b})"),
        (op::DIV, [a, b]) => format!("(ite (= {b} {zero}) {zero} (bvudiv {a} {b}))"),
        (op::SDIV, [a, b]) => format!("(ite (= {b} {zero}) {zero} (bvsdiv {a} {b}))"),
        (op::MOD, [a, b]) => format!("(ite (= {b} {zero}) {zero} (bvurem {a} {b}))"),
        (op::SMOD, [a, b]) => format!("(ite (= {b} {zero}) {zero} (bvsrem {a} {b}))"),
        (op::ADDMOD, [a, b, n]) => format!(
            "(ite (= {n} {zero}) {zero} ((_ extract 255 0) (bvurem (bvadd ((_ zero_extend 1) {a}) ((_ zero_extend 1) {b})) ((_ zero_extend 1) {n}))))"
        ),
        (op::MULMOD, [a, b, n]) => format!(
            "(ite (= {n} {zero}) {zero} ((_ extract 255 0) (bvurem (bvmul ((_ zero_extend 256) {a}) ((_ zero_extend 256) {b})) ((_ zero_extend 256) {n}))))"
        ),
        (op::SIGNEXTEND, [size, value]) => {
            // Shifting the sign byte to the top and back arithmetically extends it.
            let shift = format!(
                "(bvsub {} (bvmul {} {size}))",
                smt_word(U256::from(248)),
                smt_word(U256::from(8))
            );
            format!(
                "(ite (bvuge {size} {}) {value} (bvashr (bvshl {value} {shift}) {shift}))",
                smt_word(U256::from(31))
            )
        }
        (op::LT, [a, b]) => bool_word(format!("(bvult {a} {b})")),
        (op::GT, [a, b]) => bool_word(format!("(bvugt {a} {b})")),
        (op::SLT, [a, b]) => bool_word(format!("(bvslt {a} {b})")),
        (op::SGT, [a, b]) => bool_word(format!("(bvsgt {a} {b})")),
        (op::EQ, [a, b]) => bool_word(format!("(= {a} {b})")),
        (op::ISZERO, [a]) => bool_word(format!("(= {a} {zero})")),
        (op::AND, [a, b]) => format!("(bvand {a} {b})"),
        (op::OR, [a, b]) => format!("(bvor {a} {b})"),
        (op::XOR, [a, b]) => format!("(bvxor {a} {b})"),
        (op::NOT, [a]) => format!("(bvnot {a})"),
        (op::SHL, [shift, value]) => {
            format!("(ite (bvuge {shift} {shift_limit}) {zero} (bvshl {value} {shift}))")
        }
        (op::SHR, [shift, value]) => {
            format!("(ite (bvuge {shift} {shift_limit}) {zero} (bvlshr {value} {shift}))")
        }
        (op::SAR, [shift, value]) => format!(
            "(ite (bvuge {shift} {shift_limit}) (bvashr {value} {}) (bvashr {value} {shift}))",
            smt_word(U256::from(255))
        ),
        (op::BYTE, [index, value]) => format!(
            "(ite (bvuge {index} {}) {zero} (bvand (bvlshr {value} (bvmul (bvsub {} {index}) {})) {}))",
            smt_word(U256::from(32)),
            smt_word(U256::from(31)),
            smt_word(U256::from(8)),
            smt_word(U256::from(0xff))
        ),
        _ => return None,
    })
}

/// An over-approximation of [`smt_opcode`] for `symbol = opcode(operands)` when the opcode
/// multiplies, divides or reduces by an operand that is not a constant: an application of an
/// uninterpreted function, conjoined with laws the exact operation obeys (zero and one operands,
/// `x % n < n`, `x / n <= x`), and the function's name. Bit-vector division and wide
/// multiplication are slow to decide; the laws often suffice.
pub fn smt_opcode_abstract(
    opcode: u8,
    operands: &[&str],
    symbol: &str,
) -> Option<(String, &'static str)> {
    let zero = smt_word(U256::ZERO);
    let one = smt_word(U256::ONE);
    let constant = |term: &str| term.starts_with('#');
    let (function, laws) = match (opcode, operands) {
        (op::MUL, [a, b]) if !constant(a) && !constant(b) => (
            "umul",
            format!(
                "(=> (or (= {a} {zero}) (= {b} {zero})) (= {symbol} {zero})) (=> (= {a} {one}) (= {symbol} {b})) (=> (= {b} {one}) (= {symbol} {a})) (= {symbol} (umul {b} {a}))"
            ),
        ),
        (op::DIV, [a, b]) if !constant(b) => (
            "udiv",
            format!(
                "(=> (= {b} {zero}) (= {symbol} {zero})) (bvule {symbol} {a}) (=> (= {b} {one}) (= {symbol} {a}))"
            ),
        ),
        (op::MOD, [a, b]) if !constant(b) => (
            "umod",
            format!(
                "(=> (= {b} {zero}) (= {symbol} {zero})) (=> (distinct {b} {zero}) (bvult {symbol} {b})) (bvule {symbol} {a}) (=> (bvult {a} {b}) (= {symbol} {a}))"
            ),
        ),
        (op::ADDMOD | op::MULMOD, [a, b, n]) if !constant(n) || !constant(a) || !constant(b) => (
            if opcode == op::ADDMOD { "addmod3" } else { "mulmod3" },
            format!(
                "(=> (= {n} {zero}) (= {symbol} {zero})) (=> (distinct {n} {zero}) (bvult {symbol} {n}))"
            ),
        ),
        _ => return None,
    };
    let call = format!("({function} {})", operands.join(" "));
    Some((format!("(and (= {symbol} {call}) {laws})"), function))
}

/// The SMT-LIB literal of a 256-bit word.
pub fn smt_word(value: U256) -> String {
    format!("#x{value:064x}")
}

fn div(a: Word, b: Word) -> Word {
    if b.is_zero() { Word::ZERO } else { a.wrapping_div(b) }
}

fn rem(a: Word, b: Word) -> Word {
    if b.is_zero() { Word::ZERO } else { a.wrapping_rem(b) }
}

/// Sign-extends `value` from its byte `ext`, counted from the least significant byte, as
/// `SIGNEXTEND` does.
pub fn signextend(ext: Word, value: Word) -> Word {
    if ext < Word::from(31) {
        let bit_index = (8 * ext.wrapping_to::<u64>() + 7) as usize;
        let mask = (Word::ONE << bit_index) - Word::ONE;
        if value.bit(bit_index) { value | !mask } else { value & mask }
    } else {
        value
    }
}

fn byte(index: Word, value: Word) -> Word {
    let index = index.saturating_to::<usize>();
    if index < 32 { Word::from(value.byte(31 - index)) } else { Word::ZERO }
}

fn shl(shift: Word, value: Word) -> Word {
    let shift = shift.saturating_to::<usize>();
    if shift < 256 { value << shift } else { Word::ZERO }
}

fn shr(shift: Word, value: Word) -> Word {
    let shift = shift.saturating_to::<usize>();
    if shift < 256 { value >> shift } else { Word::ZERO }
}

fn sar(shift: Word, value: Word) -> Word {
    let shift = shift.saturating_to::<usize>();
    if shift < 256 {
        value.arithmetic_shr(shift)
    } else if value.bit(255) {
        Word::MAX
    } else {
        Word::ZERO
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(i8)]
enum Sign {
    Minus = -1,
    Zero = 0,
    Plus = 1,
}

const MIN_NEGATIVE_VALUE: Word = Word::from_limbs([
    0x0000000000000000,
    0x0000000000000000,
    0x0000000000000000,
    0x8000000000000000,
]);

const FLIPH_BITMASK_U64: u64 = 0x7fff_ffff_ffff_ffff;

#[inline]
fn i256_sign(value: &Word) -> Sign {
    if value.bit(Word::BITS - 1) {
        Sign::Minus
    } else if value.is_zero() {
        Sign::Zero
    } else {
        Sign::Plus
    }
}

#[inline]
fn i256_sign_compl(value: &mut Word) -> Sign {
    let sign = i256_sign(value);
    if sign == Sign::Minus {
        two_compl_mut(value);
    }
    sign
}

#[inline]
fn u256_remove_sign(value: &mut Word) {
    // SAFETY: A 256-bit word always has four limbs.
    unsafe {
        value.as_limbs_mut()[3] &= FLIPH_BITMASK_U64;
    }
}

#[inline]
fn two_compl_mut(value: &mut Word) {
    *value = two_compl(*value);
}

#[inline]
fn two_compl(value: Word) -> Word {
    value.wrapping_neg()
}

#[inline]
fn i256_cmp(first: &Word, second: &Word) -> Ordering {
    let first_sign = i256_sign(first);
    let second_sign = i256_sign(second);
    match first_sign.cmp(&second_sign) {
        Ordering::Equal => first.cmp(second),
        ordering => ordering,
    }
}

/// Divides two signed words, rounding towards zero, as `SDIV` does.
#[inline]
pub fn i256_div(mut first: Word, mut second: Word) -> Word {
    let second_sign = i256_sign_compl(&mut second);
    if second_sign == Sign::Zero {
        return Word::ZERO;
    }

    let first_sign = i256_sign_compl(&mut first);
    if first == MIN_NEGATIVE_VALUE && second == Word::ONE {
        return two_compl(MIN_NEGATIVE_VALUE);
    }

    let mut quotient = first / second;
    u256_remove_sign(&mut quotient);

    if (first_sign == Sign::Minus && second_sign != Sign::Minus)
        || (second_sign == Sign::Minus && first_sign != Sign::Minus)
    {
        two_compl(quotient)
    } else {
        quotient
    }
}

/// Returns the signed remainder of two words, with the sign of the dividend, as `SMOD` does.
#[inline]
pub fn i256_mod(mut first: Word, mut second: Word) -> Word {
    let first_sign = i256_sign_compl(&mut first);
    if first_sign == Sign::Zero {
        return Word::ZERO;
    }

    let second_sign = i256_sign_compl(&mut second);
    if second_sign == Sign::Zero {
        return Word::ZERO;
    }

    let mut remainder = first % second;
    u256_remove_sign(&mut remainder);

    if first_sign == Sign::Minus { two_compl(remainder) } else { remainder }
}
