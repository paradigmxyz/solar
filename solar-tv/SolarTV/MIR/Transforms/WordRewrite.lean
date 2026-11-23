import SolarTV.MIR.Ops

/-! Canonical forms for the total word operations compared by the CFG proof.
Keep each original opcode in the key. Normalization has the same typed MIR
result at every i256 operand, including overflow and wide shifts. -/

namespace SolarTV.MIR.WordRewrite
open EvmRules

inductive Scale where
  | mul | div | shl | shr
  deriving DecidableEq

inductive Key where
  | addConstant (value : Word)
  | subConstant (value : Word)
  | scale (kind : Scale) (exponent : BitVec 8)
  | double
  | highMask (complement : Bool)
  deriving DecidableEq

def Key.normalize : Key → Key
  | .subConstant amount => .addConstant (-amount)
  | .scale .mul exponent => .scale .shl exponent
  | .scale .div exponent => .scale .shr exponent
  | .double => .scale .shl 1
  | .highMask _ => .highMask false
  | key => key

def Key.result (key : Key) (input : Word) : Word :=
  match key with
  | .addConstant amount => MIR.eval .add input amount
  | .subConstant amount => MIR.eval .sub input amount
  | .scale .mul exponent => MIR.eval .mul input (BitVec.twoPow 256 exponent.toNat)
  | .scale .div exponent => MIR.eval .div input (BitVec.twoPow 256 exponent.toNat)
  | .scale .shl exponent => MIR.shift .shl input (exponent.setWidth 256)
  | .scale .shr exponent => MIR.shift .shr input (exponent.setWidth 256)
  | .double => MIR.eval .add input input
  | .highMask true => MIR.unary .not (MIR.eval .sub (MIR.shift .shl 1 input) 1)
  | .highMask false => MIR.shift .shl (-1) input

private theorem shift_amount (x : BitVec 8) :
    (x.setWidth 256).toNat = x.toNat := BitVec.toNat_setWidth_of_le (by decide)

theorem normalized (key : Key) (input : Word) :
    key.normalize.result input = key.result input := by
  cases key with
  | addConstant => rfl
  | subConstant value => exact (BitVec.sub_eq_add_neg input value).symm
  | scale kind exponent =>
    have width : exponent.toNat < 256 := exponent.isLt
    cases kind
    case mul =>
      change Word.shift .shl input _ = input * BitVec.twoPow 256 exponent.toNat
      simp only [Word.shift, shift_amount, Nat.not_le.mpr width, ite_false]
      exact BitVec.shiftLeft_eq_mul_twoPow input exponent.toNat
    case div =>
      change Word.shift .shr input _ = input / BitVec.twoPow 256 exponent.toNat
      simp only [Word.shift, shift_amount, Nat.not_le.mpr width, ite_false, BitVec.toNat_eq,
        BitVec.toNat_ushiftRight, BitVec.toNat_udiv, Nat.shiftRight_eq_div_pow,
        BitVec.toNat_twoPow_of_lt width]
    case shl => rfl
    case shr => rfl
  | double =>
    change input <<< 1 = input + input
    rw [BitVec.shiftLeft_eq_mul_twoPow]
    change input * (1#256 + 1#256) = input + input
    rw [BitVec.mul_add, BitVec.mul_one]
  | highMask complement =>
    cases complement
    · rfl
    · change Word.shift .shl (-1#256) input = ~~~(Word.shift .shl 1#256 input - 1#256)
      rw [BitVec.not_eq_neg_add, BitVec.neg_sub, BitVec.add_sub_cancel]
      by_cases wide : 256 ≤ input.toNat
      · simp only [Word.shift, wide, ite_true]
        exact (BitVec.neg_zero (n := 256)).symm
      · simp only [Word.shift, wide, ite_false, BitVec.shiftLeft_neg]

end SolarTV.MIR.WordRewrite
