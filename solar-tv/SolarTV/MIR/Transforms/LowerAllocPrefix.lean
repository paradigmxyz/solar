import SolarTV.MIR.Transforms.CoalesceAlloc

/-! Lowering the first reservation of an allocation prefix.

The concrete sequence reads the FMP, adds a constant in word arithmetic and
stores the result back. Under the existing allocation producer contract, the
addition cannot wrap and slot 0x40 is already active. It therefore implements
the abstract reservation, including its memory bytes and active size, before
any identical continuation. No heap floor or successful callee is assumed.

This is an unpriced byte-memory certificate. The source-to-prefix projection
remains trusted; it does not establish physical gas equivalence.
-/

namespace SolarTV.MIR.LowerAllocPrefix
open EvmRules CoalesceAlloc

def loweredReserve (size : Nat) (state : Result) : Result :=
  let next := readCursor state.memory + BitVec.ofNat 256 size
  {cursor := next.toNat, memory := write state.memory 64 next,
    active := max state.active 96}

theorem reservation (base size : Nat) (state : Result)
    (valid : ProducerContract base size state) :
    loweredReserve size state = run base [.reserve size] state := by
  have small : base + size < 2^256 := by
    have bound := valid.2.2.1
    omega
  have next : readCursor state.memory + BitVec.ofNat 256 size =
      BitVec.ofNat 256 (base + size) := by
    rw [valid.2.2.2.2.2.2, ← BitVec.ofNat_add]
  simp only [loweredReserve, next, BitVec.toNat_ofNat, Nat.mod_eq_of_lt small,
    run, valid.2.2.2.1, Nat.max_eq_left valid.2.2.2.2.1]

theorem contextual {α : Sort _} (base size : Nat) (state : Result)
    (valid : ProducerContract base size state) (continuation : Result → α) :
    continuation (loweredReserve size state) =
      continuation (run base [.reserve size] state) := by
  rw [reservation base size state valid]

end SolarTV.MIR.LowerAllocPrefix
