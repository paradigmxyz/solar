import Lean
import EvmRules.Bits
import SolarTV.Machine
import SolarTV.MIR.Program
import SolarTV.EVMIR.Program

/-! A kernel-only procedure for the stages of one loop-free contract.

For two concrete programs, a stage holds when, on every call, the first ends at some fuel
and both end in the same result (`stage_of_runs`). `sym_exec` proves that equation, and the
first program's end, by symbolic execution over a symbolic call and oracle:

- `dsimp` runs both programs block by block with the semantics' own equations and the
  lookups below, all definitional, so the kernel checks a computed step by evaluation and
  not by replaying a chain of rewrites. Calldata words, memory, the oracle and the word
  operations stay opaque, so both programs build the same terms for the same values.
- A branch condition that the code cannot decide is turned into a proposition over those
  terms by the `cond_*` lemmas (when the word an operation leaves is zero), and
  `split_cond` case-splits on the first one left, rewriting both programs with the case.
- A leaf closes by reflexivity when both ends agree, and by `bv_omega` when its cases
  contradict each other, as when the programs test one condition in different words.
- Shifts and `NOT` of numerals fold to numerals, as passes respell constants, and the tests
  the compiler spells an address check with (the word above the 160-bit mask, its high bits
  shifted out, the word equal to its masked self) rewrite to one proposition, so that each
  program's spelling meets the other's.

Nothing here calls `bv_decide` or `native_decide`, and `decide` only runs on closed terms inside
`dsimp`'s evaluation, so the proofs rest on the kernel and the standard axioms. A program that
loops, reads a value the code does not pin, or whose stage is false leaves a goal: the
procedure fails closed. -/

namespace SolarTV.Milestone.W1

open EvmRules Machine

/-! Branch conditions as propositions. A jump is taken on a nonzero word, so each lemma
says when the word an operation leaves is zero. -/

theorem cond_ne (a b : Word) : (Evm.ne a b = 0) = (a = b) := by
  unfold Evm.ne
  by_cases h : a = b
  · simp [h]
  · simp [h]

theorem cond_eq (a b : Word) : (Evm.eq a b = 0) = ¬(b = a) := by
  unfold Evm.eq
  by_cases h : a = b
  · simp [h]
  · have h' : ¬b = a := fun e => h e.symm
    simp [h, h']

theorem cond_sub (a b : Word) : (Evm.sub a b = 0) = (b = a) := by
  unfold Evm.sub
  apply propext
  constructor
  · intro h; bv_omega
  · intro h; bv_omega

theorem cond_lt (a b : Word) : (Evm.lt a b = 0) = ¬(a < b) := by
  unfold Evm.lt
  by_cases h : a < b
  · simp [h, BitVec.ult_iff_lt]
  · simp [h, BitVec.ult_iff_lt]

theorem cond_gt (a b : Word) : (Evm.gt a b = 0) = ¬(b < a) := by
  unfold Evm.gt
  by_cases h : b < a
  · simp [h, BitVec.ult_iff_lt]
  · simp [h, BitVec.ult_iff_lt]

theorem cond_iszero (a : Word) : (Evm.iszero a = 0) = ¬(a = 0) := by
  unfold Evm.iszero
  by_cases h : a = 0
  · simp only [h, beq_self_eq_true, Bool.cond_true, not_true_eq_false]
    decide
  · simp only [h, not_false_eq_true]
    have : (a == 0) = false := by simpa using h
    rw [this]
    decide

/-- A test compared with zero the other way round, as a fact a pass checks with `eq` reads
it (`cond_eq` puts the zero first): the same propositions as the lemmas above. -/
theorem cond_zero_lt (a b : Word) : ((0 : Word) = Evm.lt a b) = ¬(a < b) := by
  rw [← cond_lt]; exact propext ⟨Eq.symm, Eq.symm⟩

theorem cond_zero_gt (a b : Word) : ((0 : Word) = Evm.gt a b) = ¬(b < a) := by
  rw [← cond_gt]; exact propext ⟨Eq.symm, Eq.symm⟩

theorem cond_zero_eq (a b : Word) : ((0 : Word) = Evm.eq a b) = ¬(b = a) := by
  rw [← cond_eq]; exact propext ⟨Eq.symm, Eq.symm⟩

theorem cond_zero_ne (a b : Word) : ((0 : Word) = Evm.ne a b) = (a = b) := by
  rw [show ((0 : Word) = Evm.ne a b) = (Evm.ne a b = 0) from propext ⟨Eq.symm, Eq.symm⟩, cond_ne]

theorem cond_zero_iszero (a : Word) : ((0 : Word) = Evm.iszero a) = ¬(a = 0) := by
  rw [← cond_iszero]; exact propext ⟨Eq.symm, Eq.symm⟩

/-- A disjunction of tests, as the dispatcher's guards combine them, is zero when both are. -/
theorem cond_or (a b : Word) : (Evm.or a b = 0) = (a = 0 ∧ b = 0) := by
  unfold Evm.or
  exact propext BitVec.or_eq_zero_iff

/-! An address is a word of at most 160 bits, the mask's own: masking it keeps it, so a test
that a word equals its masked self holds once a test that it fits passed. The caller is the
oracle's, any address, which fits. -/

theorem and_mask {x : Word} (h : ¬ (1461501637330902918203684832716283019655932542975 : Word) < x) :
    Evm.and x 1461501637330902918203684832716283019655932542975 = x := by
  unfold Evm.and
  apply BitVec.eq_of_toNat_eq
  have hm : (1461501637330902918203684832716283019655932542975 : Word).toNat = 2 ^ 160 - 1 := rfl
  have hx : x.toNat ≤ 2 ^ 160 - 1 := by
    rw [BitVec.lt_def, hm] at h; omega
  rw [BitVec.toNat_and, hm, Nat.and_two_pow_sub_one_eq_mod]
  exact Nat.mod_eq_of_lt (by omega)

theorem mask_and {x : Word} (h : ¬ (1461501637330902918203684832716283019655932542975 : Word) < x) :
    Evm.and 1461501637330902918203684832716283019655932542975 x = x := by
  have := and_mask h
  unfold Evm.and at this ⊢
  rw [BitVec.and_comm]
  exact this

theorem caller_le_mask (o : Oracle) :
    ¬ (1461501637330902918203684832716283019655932542975 : Word) < o.caller.setWidth 256 := by
  have hm : (1461501637330902918203684832716283019655932542975 : Word).toNat = 2 ^ 160 - 1 := rfl
  have h := o.caller.isLt
  rw [BitVec.lt_def, BitVec.toNat_setWidth, hm, Nat.mod_eq_of_lt (by omega)]
  omega

/-- A width test as solar spells it, by shifting out the address bits, as the mask test. -/
theorem shr160_eq_zero (x : Word) :
    (Evm.shr 160 x = 0) = ¬ (1461501637330902918203684832716283019655932542975 : Word) < x := by
  have hm : (1461501637330902918203684832716283019655932542975 : Word).toNat = 2 ^ 160 - 1 := rfl
  have hs : Evm.shr 160 x = x >>> 160 := by
    unfold Evm.shr
    rw [show (160 : Word).ult 256 = true from rfl, Bool.cond_true, BitVec.ushiftRight_eq']
    rfl
  rw [hs, BitVec.lt_def, hm]
  apply propext
  rw [BitVec.toNat_eq, BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow]
  show x.toNat / 2 ^ 160 = 0 ↔ ¬2 ^ 160 - 1 < x.toNat
  rw [Nat.div_eq_zero_iff]
  have := x.isLt
  omega

theorem zero_eq_shr160 (x : Word) :
    (0 = Evm.shr 160 x) = ¬ (1461501637330902918203684832716283019655932542975 : Word) < x := by
  rw [← shr160_eq_zero]
  exact propext eq_comm

/-! Tests the cases decide, as the words they leave, and the arithmetic a select becomes:
`if-convert` turns a branch on a test `c` between `t` and `f` into `f + c * (t - f)`. -/

theorem lt_of_case {a b : Word} (h : a < b) : Evm.lt a b = 1 := by
  simp [Evm.lt, BitVec.ult_iff_lt, h]
theorem lt_of_not_case {a b : Word} (h : ¬(a < b)) : Evm.lt a b = 0 := by
  simp [Evm.lt, BitVec.ult_iff_lt, h]
theorem gt_of_case {a b : Word} (h : b < a) : Evm.gt a b = 1 := by
  simp [Evm.gt, BitVec.ult_iff_lt, h]
theorem gt_of_not_case {a b : Word} (h : ¬(b < a)) : Evm.gt a b = 0 := by
  simp [Evm.gt, BitVec.ult_iff_lt, h]
theorem eq_of_case {a b : Word} (h : b = a) : Evm.eq a b = 1 := by
  subst h; simp [Evm.eq]
theorem eq_of_not_case {a b : Word} (h : ¬(b = a)) : Evm.eq a b = 0 := by
  unfold Evm.eq
  rw [beq_eq_false_iff_ne.mpr (fun e => h e.symm)]
  rfl
theorem iszero_of_case {a : Word} (h : a = 0) : Evm.iszero a = 1 := by
  subst h; simp [Evm.iszero]
theorem iszero_of_not_case {a : Word} (h : ¬(a = 0)) : Evm.iszero a = 0 := by
  unfold Evm.iszero
  rw [beq_eq_false_iff_ne.mpr h]
  rfl

theorem mul_word_one (a : Word) : Evm.mul a 1 = a := BitVec.mul_one a
theorem mul_one_word (a : Word) : Evm.mul 1 a = a := BitVec.one_mul a
theorem mul_word_zero (a : Word) : Evm.mul a 0 = 0 := BitVec.mul_zero
theorem mul_zero_word (a : Word) : Evm.mul 0 a = 0 := BitVec.zero_mul
theorem add_sub_word (a b : Word) : Evm.add b (Evm.sub a b) = a := by
  unfold Evm.add Evm.sub
  bv_omega
theorem add_word_zero (a : Word) : Evm.add a 0 = a := BitVec.add_zero a
theorem add_zero_word (a : Word) : Evm.add 0 a = a := BitVec.zero_add a

/-! Commutative operations, which a pass may write either way round: the `word*Comm`
simprocs below rewrite with them towards one order of their operands, the same for both
programs. -/

theorem word_add_comm (a b : Word) : Evm.add a b = Evm.add b a := BitVec.add_comm a b
theorem word_mul_comm (a b : Word) : Evm.mul a b = Evm.mul b a := BitVec.mul_comm a b
theorem word_and_comm (a b : Word) : Evm.and a b = Evm.and b a := BitVec.and_comm a b
theorem word_or_comm (a b : Word) : Evm.or a b = Evm.or b a := BitVec.or_comm a b
theorem word_xor_comm (a b : Word) : Evm.xor a b = Evm.xor b a := BitVec.xor_comm a b

/-! Evaluation steps, each definitional. -/

/-- One iteration from a configuration the code has built. Matching only a pair keeps
`dsimp` from unfolding the iterations after a step it cannot compute yet. -/
@[defeq] theorem iterate_mk {α β : Type} (step : α × β → Option ((Halt × State) ⊕ (α × β)))
    (n : Nat) (a : α) (b : β) :
    iterate step (n + 1) (a, b) =
      match step (a, b) with
      | some (.inl result) => some result
      | some (.inr next) => iterate step n next
      | none => none := by
  rw [iterate]
  cases step (a, b) with
  | none => rfl
  | some v => cases v <;> rfl

@[defeq] theorem iterate_zero {σ : Type} (step : σ → Option ((Halt × State) ⊕ σ)) (x : σ) :
    iterate step 0 x = none := by
  rw [iterate]


/-! Effects, one equation each. A calldata word is read through `cd`, a variable the procedure
binds to `calldataWord call` first, so that the kernel never unfolds the 32-byte read while it
checks a step. -/

section effects

variable (o : Oracle) (call : Call) (s : State)

@[defeq] theorem apply_mload (offset : Word) : Effect.apply o call .mload [offset] s =
    some (some (s.memory.load offset.toNat), { s with memory := s.memory.expand offset.toNat 32 }) := rfl
@[defeq] theorem apply_mstore (offset value : Word) : Effect.apply o call .mstore [offset, value] s =
    some (none, { s with memory := s.memory.store offset.toNat value }) := rfl
@[defeq] theorem apply_mstore8 (offset value : Word) : Effect.apply o call .mstore8 [offset, value] s =
    some (none, { s with memory := s.memory.storeByte offset.toNat value }) := rfl
@[defeq] theorem apply_msize : Effect.apply o call .msize [] s = some (some s.memory.msize, s) := rfl
@[defeq] theorem apply_sload (key : Word) : Effect.apply o call .sload [key] s =
    some (some (s.storage key), s) := rfl
@[defeq] theorem apply_sstore (key value : Word) : Effect.apply o call .sstore [key, value] s =
    some (none, { s with storage := update s.storage key value }) := rfl
@[defeq] theorem apply_tload (key : Word) : Effect.apply o call .tload [key] s =
    some (some (s.transient key), s) := rfl
@[defeq] theorem apply_tstore (key value : Word) : Effect.apply o call .tstore [key, value] s =
    some (none, { s with transient := update s.transient key value }) := rfl
@[defeq] theorem apply_calldatasize : Effect.apply o call .calldatasize [] s =
    some (some (BitVec.ofNat 256 call.calldata.size), s) := rfl
@[defeq] theorem apply_callvalue : Effect.apply o call .callvalue [] s =
    some (some (BitVec.ofNat 256 call.callvalue), s) := rfl
@[defeq] theorem apply_caller : Effect.apply o call .caller [] s =
    some (some (o.caller.setWidth 256), s) := rfl
@[defeq] theorem apply_address : Effect.apply o call .address [] s =
    some (some (BitVec.ofNat 256 environment.callee), s) := rfl
@[defeq] theorem apply_returndatasize : Effect.apply o call .returndatasize [] s =
    some (some (o.env "returndatasize" s.world []), s) := rfl
@[defeq] theorem apply_keccak256 (offset size : Word) :
    Effect.apply o call .keccak256 [offset, size] s =
      let memory := s.memory.expand offset.toNat size.toNat
      some (some (o.keccak (memory.read offset.toNat size.toNat)), { s with memory }) := rfl

@[defeq] theorem apply_immutable (id : Nat) : Effect.apply o call (.immutable id) [] s =
    some (some (o.env "immutable" [] [BitVec.ofNat 256 id]), s) := rfl
@[defeq] theorem apply_codecopy (dest offset size : Word) :
    Effect.apply o call .codecopy [dest, offset, size] s =
      let memory := s.memory.write dest.toNat size.toNat (fun i => codeByte o (offset.toNat + i))
      some (none, { s with memory }) := rfl
@[defeq] theorem apply_create (value offset size : Word) :
    Effect.apply o call .create [value, offset, size] s = some (created o s [value] offset size) :=
  rfl
@[defeq] theorem apply_create2 (value offset size salt : Word) :
    Effect.apply o call .create2 [value, offset, size, salt] s =
      some (created o s [value, salt] offset size) := rfl
@[defeq] theorem apply_gas : Effect.apply o call .gas [] s =
    let world := s.world ++ [.read "gas"]
    some (some (o.env "gas" world []), { s with world }) := rfl
@[defeq] theorem apply_env (name : String) (operands : List Word) :
    Effect.apply o call (.env name) operands s = some (some (o.env name s.world operands), s) := rfl
@[defeq] theorem apply_calldatacopy (dest offset size : Word) :
    Effect.apply o call .calldatacopy [dest, offset, size] s =
      let memory := s.memory.write dest.toNat size.toNat
        (fun i => calldataByte call (offset.toNat + i))
      some (none, { s with memory }) := rfl
@[defeq] theorem apply_mcopy (dest source size : Word) :
    Effect.apply o call .mcopy [dest, source, size] s =
      let read := s.memory.expand source.toNat size.toNat
      let memory := read.write dest.toNat size.toNat (fun i => read.bytes (source.toNat + i))
      some (none, { s with memory }) := rfl
@[defeq] theorem apply_returndatacopy (dest offset size : Word) :
    Effect.apply o call .returndatacopy [dest, offset, size] s =
      if (returndataSize o s.world).toNat < offset.toNat + size.toNat then none
      else
        let memory := s.memory.write dest.toNat size.toNat
          (fun i => returndataByte o s.world (offset.toNat + i))
        some (none, { s with memory }) := rfl

/-- The memory a call sees, after the expansion of its input and output ranges. -/
def callMemory (s : State) (inOffset inSize outOffset outSize : Word) : Mem :=
  (s.memory.expand inOffset.toNat inSize.toNat).expand outOffset.toNat outSize.toNat

/-- The world after a call, which `sym_eval` unfolds. -/
def callWorld (s : State) (kind : CallKind) (seen : List Word)
    (inOffset inSize outOffset outSize : Word) : List Event :=
  s.world ++ [.call kind seen
    ((callMemory s inOffset inSize outOffset outSize).read inOffset.toNat inSize.toNat)
    s.storage s.transient]

@[defeq] theorem apply_call (gas address value inOffset inSize outOffset outSize : Word) :
    Effect.apply o call (.call .call) [gas, address, value, inOffset, inSize, outOffset, outSize] s =
      let world := callWorld s .call [gas, address, value] inOffset inSize outOffset outSize
      some (some (callFlag o world), called o .call s
        (callMemory s inOffset inSize outOffset outSize) outOffset outSize world) := rfl
@[defeq] theorem apply_callcode (gas address value inOffset inSize outOffset outSize : Word) :
    Effect.apply o call (.call .callcode)
        [gas, address, value, inOffset, inSize, outOffset, outSize] s =
      let world := callWorld s .callcode [gas, address, value] inOffset inSize outOffset outSize
      some (some (callFlag o world), called o .callcode s
        (callMemory s inOffset inSize outOffset outSize) outOffset outSize world) := rfl
@[defeq] theorem apply_delegatecall (gas address inOffset inSize outOffset outSize : Word) :
    Effect.apply o call (.call .delegatecall) [gas, address, inOffset, inSize, outOffset, outSize] s =
      let world := callWorld s .delegatecall [gas, address] inOffset inSize outOffset outSize
      some (some (callFlag o world), called o .delegatecall s
        (callMemory s inOffset inSize outOffset outSize) outOffset outSize world) := rfl
@[defeq] theorem apply_staticcall (gas address inOffset inSize outOffset outSize : Word) :
    Effect.apply o call (.call .staticcall) [gas, address, inOffset, inSize, outOffset, outSize] s =
      let world := callWorld s .staticcall [gas, address] inOffset inSize outOffset outSize
      some (some (callFlag o world), called o .staticcall s
        (callMemory s inOffset inSize outOffset outSize) outOffset outSize world) := rfl

/-- The state after a log, which `sym_eval` unfolds. -/
def logged (s : State) (offset size : Word) (topics : List Word) : State :=
  let memory := s.memory.expand offset.toNat size.toNat
  { s with memory, world := s.world ++ [.log topics (memory.read offset.toNat size.toNat)] }

@[defeq] theorem apply_log0 (offset size : Word) :
    Effect.apply o call (.log 0) [offset, size] s = some (none, logged s offset size []) := rfl
@[defeq] theorem apply_log1 (offset size t0 : Word) :
    Effect.apply o call (.log 1) [offset, size, t0] s =
      some (none, logged s offset size [t0]) := rfl
@[defeq] theorem apply_log2 (offset size t0 t1 : Word) :
    Effect.apply o call (.log 2) [offset, size, t0, t1] s =
      some (none, logged s offset size [t0, t1]) := rfl
@[defeq] theorem apply_log3 (offset size t0 t1 t2 : Word) :
    Effect.apply o call (.log 3) [offset, size, t0, t1, t2] s =
      some (none, logged s offset size [t0, t1, t2]) := rfl
@[defeq] theorem apply_log4 (offset size t0 t1 t2 t3 : Word) :
    Effect.apply o call (.log 4) [offset, size, t0, t1, t2, t3] s =
      some (none, logged s offset size [t0, t1, t2, t3]) := rfl

/-- A calldata read as a read of `cd`. Not definitional, so `simp` rewrites with its proof. -/
theorem apply_calldataload {cd : Word → Word} (hcd : cd = calldataWord call) (offset : Word) :
    Effect.apply o call .calldataload [offset] s = some (some (cd offset), s) := by
  subst hcd
  rfl

end effects

/-! EVM IR word operations, one equation each, by `rfl`. `sym_eval` rewrites a step with these
instead of unfolding `EVM.step`, whose match over every operation it would otherwise reduce on
every step of every path. `EVM.step` itself is unchanged, and an operation without an equation
here (`exchange`, or a stack too short for its operands) leaves the step unevaluated, so the
procedure fails rather than guesses. -/

section steps

variable (ctx : EVM.Context)

@[defeq] theorem step_add (a b : Word) (rest : List Word) :
    EVM.step ctx .add (a :: b :: rest) = some (Evm.add a b :: rest) := rfl
@[defeq] theorem step_mul (a b : Word) (rest : List Word) :
    EVM.step ctx .mul (a :: b :: rest) = some (Evm.mul a b :: rest) := rfl
@[defeq] theorem step_and (a b : Word) (rest : List Word) :
    EVM.step ctx .and (a :: b :: rest) = some (Evm.and a b :: rest) := rfl
@[defeq] theorem step_or (a b : Word) (rest : List Word) :
    EVM.step ctx .or (a :: b :: rest) = some (Evm.or a b :: rest) := rfl
@[defeq] theorem step_xor (a b : Word) (rest : List Word) :
    EVM.step ctx .xor (a :: b :: rest) = some (Evm.xor a b :: rest) := rfl
@[defeq] theorem step_div (a b : Word) (rest : List Word) :
    EVM.step ctx .div (a :: b :: rest) = some (Evm.div a b :: rest) := rfl
@[defeq] theorem step_sdiv (a b : Word) (rest : List Word) :
    EVM.step ctx .sdiv (a :: b :: rest) = some (Evm.sdiv a b :: rest) := rfl
@[defeq] theorem step_mod (a b : Word) (rest : List Word) :
    EVM.step ctx .mod (a :: b :: rest) = some (Evm.mod a b :: rest) := rfl
@[defeq] theorem step_smod (a b : Word) (rest : List Word) :
    EVM.step ctx .smod (a :: b :: rest) = some (Evm.smod a b :: rest) := rfl
@[defeq] theorem step_signextend (a b : Word) (rest : List Word) :
    EVM.step ctx .signextend (a :: b :: rest) = some (Evm.signextend a b :: rest) := rfl
@[defeq] theorem step_byte (a b : Word) (rest : List Word) :
    EVM.step ctx .byte (a :: b :: rest) = some (Evm.byte a b :: rest) := rfl
@[defeq] theorem step_shl (a b : Word) (rest : List Word) :
    EVM.step ctx .shl (a :: b :: rest) = some (Evm.shl a b :: rest) := rfl
@[defeq] theorem step_shr (a b : Word) (rest : List Word) :
    EVM.step ctx .shr (a :: b :: rest) = some (Evm.shr a b :: rest) := rfl
@[defeq] theorem step_sar (a b : Word) (rest : List Word) :
    EVM.step ctx .sar (a :: b :: rest) = some (Evm.sar a b :: rest) := rfl
@[defeq] theorem step_lt (a b : Word) (rest : List Word) :
    EVM.step ctx .lt (a :: b :: rest) = some (Evm.lt a b :: rest) := rfl
@[defeq] theorem step_gt (a b : Word) (rest : List Word) :
    EVM.step ctx .gt (a :: b :: rest) = some (Evm.gt a b :: rest) := rfl
@[defeq] theorem step_slt (a b : Word) (rest : List Word) :
    EVM.step ctx .slt (a :: b :: rest) = some (Evm.slt a b :: rest) := rfl
@[defeq] theorem step_sgt (a b : Word) (rest : List Word) :
    EVM.step ctx .sgt (a :: b :: rest) = some (Evm.sgt a b :: rest) := rfl
@[defeq] theorem step_eq (a b : Word) (rest : List Word) :
    EVM.step ctx .eq (a :: b :: rest) = some (Evm.eq a b :: rest) := rfl
@[defeq] theorem step_sub (a b : Word) (rest : List Word) :
    EVM.step ctx .sub (a :: b :: rest) = some (Evm.sub a b :: rest) := rfl
@[defeq] theorem step_exp (a b : Word) (rest : List Word) :
    EVM.step ctx .exp (a :: b :: rest) = some (Word.power a b.toNat :: rest) := rfl
@[defeq] theorem step_addmod (a b n : Word) (rest : List Word) :
    EVM.step ctx .addmod (a :: b :: n :: rest) = some (Evm.addmod a b n :: rest) := rfl
@[defeq] theorem step_mulmod (a b n : Word) (rest : List Word) :
    EVM.step ctx .mulmod (a :: b :: n :: rest) = some (Evm.mulmod a b n :: rest) := rfl
@[defeq] theorem step_not (a : Word) (rest : List Word) :
    EVM.step ctx .not (a :: rest) = some (Evm.not a :: rest) := rfl
@[defeq] theorem step_clz (a : Word) (rest : List Word) :
    EVM.step ctx .clz (a :: rest) = some (Evm.clz a :: rest) := rfl
@[defeq] theorem step_iszero (a : Word) (rest : List Word) :
    EVM.step ctx .iszero (a :: rest) = some (Evm.iszero a :: rest) := rfl
@[defeq] theorem step_pop (a : Word) (rest : List Word) :
    EVM.step ctx .pop (a :: rest) = some rest := rfl
@[defeq] theorem step_dup (index : Fin 256) (stack : List Word) :
    EVM.step ctx (.dup index) stack = (stack[index.val]?).map (fun value => value :: stack) := rfl
@[defeq] theorem step_swap (index : Fin 256) (a : Word) (rest : List Word) :
    EVM.step ctx (.swap index) (a :: rest) =
      (rest[index.val]?).map (fun value => value :: rest.set index.val a) := rfl
@[defeq] theorem step_calldatasize (rest : List Word) :
    EVM.step ctx .calldatasize rest = some (ctx.calldataSize :: rest) := rfl
@[defeq] theorem step_returndatasize (rest : List Word) :
    EVM.step ctx .returndatasize rest = some (ctx.returndataSize :: rest) := rfl
@[defeq] theorem step_push (value : Word) (rest : List Word) :
    EVM.step ctx (.push value) rest = some (value :: rest) := rfl

end steps

/-! Stack positions of `dup` and `swap` as numbers. -/

theorem fin256_val_0 : ((0 : Fin 256) : Nat) = 0 := rfl
theorem fin256_val_1 : ((1 : Fin 256) : Nat) = 1 := rfl
theorem fin256_val_2 : ((2 : Fin 256) : Nat) = 2 := rfl
theorem fin256_val_3 : ((3 : Fin 256) : Nat) = 3 := rfl
theorem fin256_val_4 : ((4 : Fin 256) : Nat) = 4 := rfl
theorem fin256_val_5 : ((5 : Fin 256) : Nat) = 5 := rfl
theorem fin256_val_6 : ((6 : Fin 256) : Nat) = 6 := rfl
theorem fin256_val_7 : ((7 : Fin 256) : Nat) = 7 := rfl
theorem fin256_val_8 : ((8 : Fin 256) : Nat) = 8 := rfl
theorem fin256_val_9 : ((9 : Fin 256) : Nat) = 9 := rfl
theorem fin256_val_10 : ((10 : Fin 256) : Nat) = 10 := rfl
theorem fin256_val_11 : ((11 : Fin 256) : Nat) = 11 := rfl
theorem fin256_val_12 : ((12 : Fin 256) : Nat) = 12 := rfl
theorem fin256_val_13 : ((13 : Fin 256) : Nat) = 13 := rfl
theorem fin256_val_14 : ((14 : Fin 256) : Nat) = 14 := rfl
theorem fin256_val_15 : ((15 : Fin 256) : Nat) = 15 := rfl

open Lean Meta Simp in
/-- A list spelled out as a program constant's field, unfolded to its spine; any other list is
left as it is, so that a stack is only read once its words are computed. A long list literal
elaborates to chunks bound with `have`, which unfold to the same spine. -/
def programSpine? (xs : Expr) : MetaM (Option Expr) := do
  let xs := xs.consumeMData
  if xs.isAppOfArity ``List.cons 3 || xs.isAppOfArity ``List.nil 1 then return some xs
  if xs.isLet then return some (← whnfD xs)
  let fn := xs.getAppFn
  let some name := fn.constName? | return none
  unless (← getEnv).isProjectionFn name do return none
  let some arg := xs.getAppArgs.back? | return none
  unless arg.consumeMData.isConst || (arg.getAppFn.isConst &&
      (← getEnv).isConstructor arg.getAppFn.constName!) do
    return none
  return some (← whnfD xs)

open Lean Meta Simp in
/-- The element at a numeral index of a list the code spells out, as `some x` or `none`. It
reduces only the spine of a program's list, so a program stays a constant in the goal and only
the block a step enters is unfolded, and never evaluates a list it does not see. -/
dsimproc_decl listLookup ((_ : List _)[_]?) := fun e => do
  let_expr GetElem?.getElem? _ _ elem _ _ xs i := e | return .continue
  let some n ← Nat.fromExpr? i | return .continue
  let some spine ← programSpine? xs | return .continue
  let mut spine := spine
  for _ in [0:n] do
    let_expr List.cons _ _ tail := spine | return .continue
    let some tail' ← programSpine? tail | return .continue
    spine := tail'
  match_expr spine with
  | List.cons _ head _ => return .done (← mkAppOptM ``Option.some #[elem, head])
  | List.nil _ => return .done (← mkAppOptM ``Option.none #[elem])
  | _ => return .continue

open Lean Meta Simp in
/-- The length of a list the code spells out, as a numeral, such as a program's block count,
which a dynamic jump past the last block compares with. -/
dsimproc_decl listLength (List.length _) := fun e => do
  let_expr List.length _ xs := e | return .continue
  let some spine ← programSpine? xs | return .continue
  let mut spine := spine
  let mut n := 0
  repeat
    match_expr spine with
    | List.cons _ _ tail =>
      let some tail' ← programSpine? tail | return .continue
      spine := tail'
      n := n + 1
    | List.nil _ => break
    | _ => return .continue
  return .done (mkNatLit n)

open Lean Meta Simp in
/-- The entry function of a module the code spells out. -/
dsimproc_decl moduleEntry (SolarTV.MIR.Lowered.Module.entry _) := fun e => do
  let e' ← whnfD e
  if (← Nat.fromExpr? e').isSome then return .done e' else return .continue

open Lean Meta Simp in
/-- The parameters of a function the code spells out. -/
dsimproc_decl functionParams (SolarTV.MIR.Lowered.Function.params _) := fun e => do
  let e' ← whnfD e
  match_expr e' with
  | List.nil _ => return .done e'
  | List.cons _ _ _ => return .done e'
  | _ => return .continue

open Lean Meta Simp in
/-- An `if` whose condition the code fixes: its `Decidable` instance evaluates to `isTrue` or
`isFalse`, so the `if` is its branch by definition. Conditions on the call stay. -/
dsimproc_decl decideIte (ite _ _ _) := fun e => do
  let_expr ite _ c inst yes no := e | return .continue
  if c.hasFVar || c.hasMVar then return .continue
  let inst ← whnfD inst
  if inst.isAppOf ``Decidable.isTrue then return .visit yes
  if inst.isAppOf ``Decidable.isFalse then return .visit no
  return .continue

open Lean Meta in
/-- The value of a numeral word. -/
def wordLit? (e : Expr) : MetaM (Option Nat) := do
  match ← getBitVecValue? e with
  | some ⟨256, v⟩ => return some v.toNat
  | _ => return none

open Lean Meta Simp in
/-- A shift or `NOT` of numerals as the numeral it computes, as a pass spells a constant
(`compact-pushes` pushes the address mask as `shr(96, not(0))`), so that the lemmas about
the constant apply. The kernel checks the value by evaluation. -/
dsimproc_decl foldNot (Evm.not _) := fun e => do
  let_expr Evm.not a := e | return .continue
  let some x ← wordLit? a | return .continue
  return .done (← mkNumeral (mkConst ``EvmRules.Word) (2 ^ 256 - 1 - x))

open Lean Meta Simp in
/-- See `foldNot`. -/
dsimproc_decl foldShl (Evm.shl _ _) := fun e => do
  let_expr Evm.shl s v := e | return .continue
  let some s ← wordLit? s | return .continue
  let some v ← wordLit? v | return .continue
  return .done (← mkNumeral (mkConst ``EvmRules.Word) (if s < 256 then (v <<< s) % 2 ^ 256 else 0))

open Lean Meta Simp in
/-- See `foldNot`. -/
dsimproc_decl foldShr (Evm.shr _ _) := fun e => do
  let_expr Evm.shr s v := e | return .continue
  let some s ← wordLit? s | return .continue
  let some v ← wordLit? v | return .continue
  return .done (← mkNumeral (mkConst ``EvmRules.Word) (if s < 256 then v >>> s else 0))

open Lean Elab Tactic Meta in
/-- Case-split on the first `if` of the goal whose condition is closed, as `hc`, with `hcs` its
symmetric form when it is an equation or its negation, so that either program's orientation
of a test rewrites. A positive equation is oriented to rewrite its open side into its closed
side, and a side that contains the other into the other, as a word equal to its masked self
(an ABI address check) rewrites the masked word into the word: the other way would loop. It
fails when no such condition is left, or when one is already a hypothesis, which means the
rewrite with it did not reach its `if`. -/
elab "split_cond" : tactic => withMainContext do
  let target ← instantiateMVars (← getMainTarget)
  let some cond := (target.find? fun e =>
      e.isAppOfArity ``ite 5 && !(e.getArg! 1).hasLooseBVars && !(e.getArg! 1).hasMVar).map
      (·.getArg! 1)
    | throwError "split_cond: no closed condition in the goal"
  for decl in ← getLCtx do
    if decl.isImplementationDetail then continue
    let ty ← instantiateMVars decl.type
    if ty == cond || ty == mkNot cond then
      throwError "split_cond: the case {cond} is a hypothesis but its condition remains"
  let stx ← Term.exprToSyntax cond
  let hc := mkIdent `hc
  let hcs := mkIdent `hcs
  let closed (e : Expr) : Bool := !e.hasFVar && !e.hasMVar
  let isEq := cond.isAppOfArity ``Eq 3
  -- x = f x rewrites as f x = x, which ends.
  let pos ← if isEq && (closed (cond.getArg! 1) || (cond.getArg! 1).occurs (cond.getArg! 2)) then
      `(tactic| (replace $hc:ident := Eq.symm $hc; have $hcs:ident := $hc))
    else `(tactic| have $hcs:ident := $hc)
  let neg ← if isEq then `(tactic| have $hcs:ident := Ne.symm $hc)
    else `(tactic| have $hcs:ident := $hc)
  evalTactic (← `(tactic| by_cases $hc:ident : $stx))
  let goals ← getGoals
  let (yes, no, rest) ← match goals with
    | yes :: no :: rest => pure (yes, no, rest)
    | _ => throwError "split_cond: by_cases did not give two cases"
  setGoals [yes]
  evalTactic pos
  let yes' ← getGoals
  setGoals [no]
  evalTactic neg
  setGoals (yes' ++ (← getGoals) ++ rest)

open Lean Meta Simp in
/-- A low mask of a natural number as its remainder, `a &&& (2^k - 1)` as `a % 2^k`, for a
literal mask on either side, so that `omega` reads the masks of narrow integers. -/
simproc_decl natMask ((_ : Nat) &&& _) := fun e => do
  let_expr HAnd.hAnd _ _ _ _ a b := e | return .continue
  let (x, mask, swapped) ← match ← Nat.fromExpr? b with
    | some n => pure (a, n, false)
    | none =>
      match ← Nat.fromExpr? a with
      | some n => pure (b, n, true)
      | none => return .continue
  let k := (mask + 1).log2
  unless 2 ^ k == mask + 1 do return .continue
  let rhs ← mkAppM ``HMod.hMod #[x, mkNatLit (mask + 1)]
  -- x &&& 2^k - 1 = x % 2^k, with both sides' literals computed by the kernel.
  let low ← mkExpectedTypeHint (mkApp2 (mkConst ``Nat.and_two_pow_sub_one_eq_mod) x (mkNatLit k))
    (← mkEq (← mkAppM ``HAnd.hAnd #[x, mkNatLit mask]) rhs)
  let proof ← if swapped then mkEqTrans (← mkAppM ``Nat.and_comm #[a, b]) low else pure low
  return .done { expr := rhs, proof? := some proof }

/-- Cases that contradict each other: the words' bitwise and modular operations as
bit-vector ones, a word combined with itself or added to zero as the word it leaves, as
numbers, low masks as remainders and right shifts as quotients, then `omega`. -/
macro "sym_omega" : tactic => `(tactic| (
  (try simp only [Evm.and, Evm.or, Evm.xor, Evm.add, Evm.sub, BitVec.xor_self, BitVec.and_self,
    BitVec.or_self, BitVec.zero_add, BitVec.add_zero] at *)
  -- The rewrites above may leave a false case already.
  all_goals (
    (try simp -implicitDefEqProofs only [bitvec_to_nat] at *)
    (try simp only [BitVec.toNat_and, toNat_shr, BitVec.toNat_ofNat, Nat.reducePow, Nat.reduceMod,
      natMask] at *)
    omega)))

/-- Two words equal under the cases: with a word combined with itself or added to zero as the
word it leaves, as numbers, with low masks as remainders and right shifts as quotients, by
`omega`; else equal up to the order of the operands of AND, OR, XOR and ADD, as the stack
scheduler commutes them (`ac_rfl`). -/
macro "sym_word" : tactic => `(tactic| (
  (try simp only [Evm.and, Evm.or, Evm.xor, Evm.add, Evm.sub, BitVec.xor_self, BitVec.and_self,
    BitVec.or_self, BitVec.zero_add, BitVec.add_zero] at *)
  -- The rewrites above may close the equation already.
  all_goals first
    | (apply BitVec.eq_of_toNat_eq
       (try simp -implicitDefEqProofs only [bitvec_to_nat] at *)
       (try simp only [BitVec.toNat_and, toNat_shr, BitVec.toNat_ofNat, Nat.reducePow, Nat.reduceMod,
         natMask] at *)
       omega)
    | ac_rfl))

open Lean Meta Elab Tactic in
/-- A proof that two ends are equal where they differ only in words: a parallel walk of both
terms that keeps what they share, proves each pair of differing words with `sym_word`, and
puts the proof together by congruence. -/
partial def agree (a b : Expr) : TacticM Expr := do
  -- Annotations, such as a case's, are no part of the terms.
  let a := a.consumeMData
  let b := b.consumeMData
  if a == b then return ← mkEqRefl a
  let ty ← whnfR (← inferType a)
  if ty.isAppOfArity ``BitVec 1 then
    let m ← mkFreshExprSyntheticOpaqueMVar (← mkEq a b)
    -- Without error recovery, so that a pair it cannot prove fails rather than closes with an
    -- error's `sorry`.
    let rest ← Tactic.run m.mvarId! (withoutRecover (evalTactic (← `(tactic| sym_word))))
    let proof ← instantiateMVars m
    unless rest.isEmpty && !proof.hasSorry do
      throwError "sym_agree: cannot prove the words equal:{indentExpr a}\nand{indentExpr b}"
    return proof
  match a, b with
  | .app f x, .app g y =>
    if x == y then return ← mkCongrFun (← agree f g) x
    let hx ← agree x y
    if f == g then return ← mkCongrArg f hx
    mkCongr (← agree f g) hx
  | _, _ =>
    if ← withReducible (isDefEq a b) then return ← mkEqRefl a
    throwError "sym_agree: the ends differ beyond words:{indentExpr a}\nand{indentExpr b}"

open Lean Meta Elab Tactic in
/-- Closes an equation of two ends that differ only in words a pass rewrote, as in an e-graph
or CSE stage, each pair proved equal from the cases. -/
elab "sym_agree" : tactic => withMainContext do
  let goal ← getMainGoal
  let some (_, l, r) := (← instantiateMVars (← goal.getType)).consumeMData.eq?
    | throwError "sym_agree: the goal is not an equation"
  goal.assign (← agree l r)
  replaceMainGoal []

/-- Two ends that differ in a select's arithmetic or in the order of a commutative operation's
operands, as stack scheduling may compute them: the tests the cases decide as their words, the
arithmetic on zero and one as the arm it picks, and the operands of additions, products and
bitwise operations in one order. -/
macro "sym_select" : tactic => `(tactic| (
  (try simp (disch := first | assumption | (apply Classical.not_not.mp; assumption))
    only [lt_of_case, lt_of_not_case, gt_of_case, gt_of_not_case, eq_of_case, eq_of_not_case,
      iszero_of_case, iszero_of_not_case, mul_word_one, mul_one_word, mul_word_zero,
      mul_zero_word, add_sub_word, add_word_zero, add_zero_word])
  (try simp only [Evm.add, Evm.mul, Evm.and, Evm.or, Evm.xor, BitVec.add_comm, BitVec.mul_comm,
    BitVec.and_comm, BitVec.or_comm, BitVec.xor_comm])
  all_goals first
    | with_reducible rfl
    | sym_agree))

/-- The proposition a hypothesis denies, for `¬ p` and `a ≠ b`. -/
def deniedProp? (ty : Lean.Expr) : Option Lean.Expr :=
  if let some p := ty.not? then some p.consumeMData
  else if ty.isAppOfArity ``Ne 3 then
    some (Lean.mkApp3 (Lean.mkConst ``Eq ty.getAppFn.constLevels!) (ty.getArg! 0) (ty.getArg! 1)
      (ty.getArg! 2))
  else none

open Lean Meta Elab Tactic in
/-- Closes a goal whose hypotheses contradict each other as they are written: `False`, a
hypothesis `a ≠ a`, or a proposition and its negation. Unlike `contradiction`, it compares
the hypotheses' terms and never reduces them, so its cost is bounded by their size: on a path
whose ends differ, `contradiction` reduced the word terms of its hypotheses to weak head
normal form for minutes (mulx's `spell-width-tests` stage with a shift edited), so that the
procedure ran out of heartbeats rather than reporting the mismatch. -/
elab "sym_absurd" : tactic => withMainContext do
  let goal ← getMainGoal
  let target ← instantiateMVars (← goal.getType)
  let mut props : Array (Expr × Expr) := #[]
  for decl in ← getLCtx do
    if decl.isImplementationDetail then continue
    props := props.push (decl.toExpr, (← instantiateMVars decl.type).consumeMData)
  for (h, ty) in props do
    if ty.isConstOf ``False then
      goal.assign (← mkFalseElim target h)
      replaceMainGoal []
      return
    let some p := deniedProp? ty | continue
    if let some (_, a, b) := p.eq? then
      if a.consumeMData == b.consumeMData then
        -- h rfl : False
        goal.assign (← mkFalseElim target (mkApp h (← mkEqRefl a)))
        replaceMainGoal []
        return
    for (h', ty') in props do
      if ty' == p then
        -- absurd h' h
        goal.assign (← mkAbsurd target h' h)
        replaceMainGoal []
        return
  throwError "sym_absurd: no two hypotheses contradict each other"

/-- A path whose cases contradict each other once the operands of additions, products and
bitwise operations are in one order: the stack scheduler commutes them, so the two programs'
tests of one word can differ in that order alone (`mul a b` against `mul b a`). -/
macro "sym_comm_contra" : tactic => `(tactic| (
  (try simp only [Evm.add, Evm.mul, Evm.and, Evm.or, Evm.xor, BitVec.add_comm, BitVec.mul_comm,
    BitVec.and_comm, BitVec.or_comm, BitVec.xor_comm] at *)
  first
    | sym_absurd
    | (simp_all; done)))

open Lean Meta Simp in
/-- The operands of a commutative word operation `op` in one order, the structural order of
their terms, proved by `comm`. Permutation lemmas would order them the same way for both
programs too, but `simp` compares terms for them at a cost that grows with the goal (over
100 s for one call on mulx's `spell-width-tests` stage); this compares the terms as they
are. -/
def wordCommStep (op comm : Name) (e : Expr) : SimpM Step := do
  unless e.isAppOfArity op 2 do return .continue
  let a := e.appFn!.appArg!
  let b := e.appArg!
  unless b.lt a do return .continue
  -- op a b = op b a
  return .visit { expr := mkApp2 (mkConst op) b a, proof? := some (mkApp2 (mkConst comm) a b) }

simproc_decl wordAddComm (Evm.add _ _) := wordCommStep ``Evm.add ``word_add_comm
simproc_decl wordMulComm (Evm.mul _ _) := wordCommStep ``Evm.mul ``word_mul_comm
simproc_decl wordAndComm (Evm.and _ _) := wordCommStep ``Evm.and ``word_and_comm
simproc_decl wordOrComm (Evm.or _ _) := wordCommStep ``Evm.or ``word_or_comm
simproc_decl wordXorComm (Evm.xor _ _) := wordCommStep ``Evm.xor ``word_xor_comm

open Lean Meta Simp in
/-- `not` of a literal as the literal it leaves, as `compact-pushes` spells a wide constant
(`push 255; not` for `~0xff`), so that both programs push the same word. -/
dsimproc_decl evmNotLit (Evm.not _) := fun e => do
  let_expr Evm.not a := e | return .continue
  let_expr OfNat.ofNat ty lit _ := a | return .continue
  let some n := lit.rawNatLit? | return .continue
  return .done (← mkNumeral ty (~~~(BitVec.ofNat 256 n)).toNat)

/-- The definitional evaluation of both layers. -/
macro "sym_eval" : tactic => `(tactic|
  dsimp only [iterate_mk, iterate_zero, listLookup, listLength, moduleEntry, functionParams,
    foldNot, foldShl, foldShr,
    MIR.Lowered.Module.run, MIR.Lowered.moduleBlock, MIR.Lowered.segment,
    MIR.Lowered.splitCall, MIR.Lowered.bindResult, MIR.Lowered.bindResults,
    MIR.Lowered.bindExtras, MIR.Lowered.publishResults, MIR.Lowered.publishWords, MIR.Lowered.bindPhis,
    MIR.Lowered.bindParams, MIR.Lowered.execInsts, MIR.Lowered.Operand.eval,
    MIR.Lowered.Env.set, MIR.Lowered.switchTarget, MIR.Lowered.halt, MIR.Lowered.evalPure,
    MIR.Lowered.lowBits, MIR.Lowered.selectWord, MIR.Lowered.Function.start,
    MIR.Lowered.Module.startState, List.find?_cons, List.find?_nil, Nat.reduceBEq,
    EVM.Program.run, EVM.Program.block, EVM.Program.exec, EVM.Program.stepOp,
    EVM.Program.output, EVM.Program.context, EVM.stepChecked,
    step_add, step_mul, step_and, step_or, step_xor, step_div, step_sdiv, step_mod, step_smod,
    step_signextend, step_byte, step_shl, step_shr, step_sar, step_lt, step_gt, step_slt, step_sgt,
    step_eq, step_sub, step_exp, step_addmod, step_mulmod, step_not, step_clz, step_iszero,
    step_pop, step_dup, step_swap, step_calldatasize, step_returndatasize, step_push, apply_mload,
    apply_mstore, apply_mstore8, apply_msize, apply_sload, apply_sstore, apply_tload,
    apply_tstore, apply_calldatasize, apply_callvalue, apply_caller, apply_address,
    apply_returndatasize, apply_keccak256, apply_log0, apply_log1, apply_log2, apply_log3,
    apply_log4, logged, apply_gas, apply_immutable, apply_codecopy, apply_create,
    apply_create2, created, apply_env, apply_calldatacopy, apply_mcopy,
    apply_returndatacopy, apply_call, apply_callcode, apply_delegatecall,
    apply_staticcall, called, returndataSize, BitVec.reduceToNat,
    List.map, List.take, List.drop, List.set, List.length, List.find?, Nat.reduceBEq,
    Except.map, Option.map,
    Option.isSome, Nat.reduceAdd, Nat.reduceSub, Nat.reduceMul, evmNotLit,
    fin256_val_0, fin256_val_1, fin256_val_2, fin256_val_3, fin256_val_4, fin256_val_5,
    fin256_val_6, fin256_val_7, fin256_val_8, fin256_val_9, fin256_val_10, fin256_val_11,
    fin256_val_12, fin256_val_13, fin256_val_14, fin256_val_15])

set_option hygiene false in
/-- Branch conditions as propositions, decided by the cases split so far. -/
macro "sym_cond" : tactic => `(tactic|
  simp only [*, apply_calldataload _ _ _ hcd, Mem.load_expand, Mem.load_write_of_disjoint,
    Mem.expand_store, Mem.load_store_same,
    cond_ne, cond_eq, cond_sub, cond_lt, cond_gt, cond_iszero, cond_or,
    cond_zero_lt, cond_zero_gt, cond_zero_eq, cond_zero_ne, cond_zero_iszero,
    wordAddComm, wordMulComm, wordAndComm, wordOrComm, wordXorComm, and_mask,
    mask_and, caller_le_mask, shr160_eq_zero, zero_eq_shr160, and_self, and_true, true_and, and_false, false_and,
    ite_true, ite_false,
    not_false_eq_true, not_true_eq_false, Classical.not_not, ↓reduceIte, Nat.reduceGT, Nat.reduceLT,
    Nat.reduceEqDiff, Nat.reduceLeDiff, Nat.reduceAdd, Nat.reduceSub, Nat.lt_irrefl,
    Bool.false_eq_true, gt_iff_lt, true_or, or_true])

/-- Evaluate and decide conditions until neither changes the goal. -/
macro "sym_norm" : tactic => `(tactic| repeat (first | sym_eval | sym_cond))

set_option hygiene false in
/-- Symbolic execution to one result: evaluate, decide what the cases decide, split on the
next condition, and close each leaf by reflexivity or a contradiction between its cases. A leaf
it cannot close fails with its goal, the two ends on that path. -/
macro "sym_exec" : tactic => `(tactic| (
  obtain ⟨cd, hcd⟩ : ∃ cd, cd = calldataWord call := ⟨_, rfl⟩
  sym_norm
  repeat' (split_cond <;> sym_norm)
  all_goals first
    | with_reducible rfl
    | (exfalso; bv_omega)
    | (exfalso; sym_omega)
    | (exfalso; sym_comm_contra)
    | sym_select
    | sym_agree
    | fail "sym_exec: the runs end in different results on this path, or the procedure cannot tell"))

end SolarTV.Milestone.W1
