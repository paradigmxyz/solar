import gen.Stages

/-! Each exported artifact of w1 run in Lean on concrete calls.

Every artifact, from the first lowered MIR to the final EVM IR, observes the same result on
each call below: a valid addition returns the sum, an overflowing one reverts with
`Panic(0x11)`, a call with value, an unknown selector and short calldata revert empty. The
results are checked by the kernel (`decide +kernel`), with no native evaluation; the oracle is
irrelevant here, since w1 reads nothing from it. -/

namespace SolarTV.Milestone.W1.Sanity

open SolarTV Machine EvmRules

/-- An oracle that answers zero, for solidity-lean's test caller. -/
def oracle : Oracle :=
  ⟨fun _ _ _ => 0, fun _ => 0, fun _ _ => 0, fun _ _ => 0, fun _ => 0,
    BitVec.ofNat 160 environment.caller⟩

/-- The big-endian 32 bytes of a word. -/
def word (n : Nat) : List UInt8 := (List.range 32).map fun i => UInt8.ofNat (n / 256 ^ (31 - i))

/-- A call of `f(a, b)`, selector `0x13d1aa2e`, with `value` wei and the selector `selector`. -/
def call (a b : Nat) (value : Nat := 0) (selector : List UInt8 := [0x13, 0xd1, 0xaa, 0x2e]) :
    Call :=
  ⟨⟨(selector ++ word a ++ word b).toArray⟩, value, []⟩

/-- A call whose calldata is only the selector and one argument. -/
def short : Call := ⟨⟨([0x13, 0xd1, 0xaa, 0x2e] ++ word 1).toArray⟩, 0, []⟩

/-- Success and output of an end, as `observe` gives them. -/
def outcome (c : Call) : Option (Halt × State) → Option (Bool × List Nat)
  | some r => some ((observe c r.2 r.1).success, (observe c r.2 r.1).output)
  | none => none

def mir (m : MIR.Lowered.Module) (c : Call) : Option (Bool × List Nat) :=
  outcome c (MIR.Lowered.Module.run oracle m c 16)

def evm (p : EVM.Program.Program) (c : Call) : Option (Bool × List Nat) :=
  outcome c (EVM.Program.run oracle p c 32)

def sum : Option (Bool × List Nat) := some (true, List.replicate 31 0 ++ [3])

def panic : Option (Bool × List Nat) :=
  some (false, [0x4e, 0x48, 0x7b, 0x71] ++ List.replicate 31 0 ++ [0x11])

def empty : Option (Bool × List Nat) := some (false, [])

def max : Nat := 2 ^ 256 - 1

/-! The first lowered MIR. -/

theorem stage0_sum : mir Export.Main.stage0 (call 1 2) = sum := by decide +kernel
theorem stage0_overflow : mir Export.Main.stage0 (call max 1) = panic := by decide +kernel
theorem stage0_value : mir Export.Main.stage0 (call 1 2 (value := 1)) = empty := by decide +kernel
theorem stage0_selector :
    mir Export.Main.stage0 (call 1 2 (selector := [0, 0, 0, 0])) = empty := by decide +kernel
theorem stage0_short : mir Export.Main.stage0 short = empty := by decide +kernel

/-! The scheduled EVM IR and the output of every EVM IR pass. -/

theorem sched_sum : evm Export.Main.sched (call 1 2) = sum := by decide +kernel
theorem sched_overflow : evm Export.Main.sched (call max 1) = panic := by decide +kernel
theorem sched_value : evm Export.Main.sched (call 1 2 (value := 1)) = empty := by decide +kernel
theorem sched_selector :
    evm Export.Main.sched (call 1 2 (selector := [0, 0, 0, 0])) = empty := by decide +kernel
theorem sched_short : evm Export.Main.sched short = empty := by decide +kernel

theorem evm1_sum : evm Export.Main.evm1 (call 1 2) = sum := by decide +kernel
theorem evm1_overflow : evm Export.Main.evm1 (call max 1) = panic := by decide +kernel
theorem evm2_sum : evm Export.Main.evm2 (call 1 2) = sum := by decide +kernel
theorem evm2_overflow : evm Export.Main.evm2 (call max 1) = panic := by decide +kernel
theorem evm3_sum : evm Export.Main.evm3 (call 1 2) = sum := by decide +kernel
theorem evm3_overflow : evm Export.Main.evm3 (call max 1) = panic := by decide +kernel
theorem evm4_sum : evm Export.Main.evm4 (call 1 2) = sum := by decide +kernel
theorem evm4_overflow : evm Export.Main.evm4 (call max 1) = panic := by decide +kernel
theorem evm4_value : evm Export.Main.evm4 (call 1 2 (value := 1)) = empty := by decide +kernel
theorem evm4_selector :
    evm Export.Main.evm4 (call 1 2 (selector := [0, 0, 0, 0])) = empty := by decide +kernel
theorem evm4_short : evm Export.Main.evm4 short = empty := by decide +kernel

/-! The checks tell programs apart: entering `@f` directly skips the value and selector
checks, and dropping the panic block makes an overflow stop with success instead. -/

/-- `stage0` entered at `@f` instead of the dispatcher. -/
def withoutDispatch : MIR.Lowered.Module := { Export.Main.stage0 with entry := 0 }

theorem withoutDispatch_value : mir withoutDispatch (call 1 2 (value := 1)) = sum := by
  decide +kernel

/-- `evm4` without its last block, the panic. -/
def withoutPanic : EVM.Program.Program := ⟨Export.Main.evm4.blocks.take 2⟩

theorem withoutPanic_overflow : evm withoutPanic (call max 1) = some (true, []) := by
  decide +kernel

/-! The bytecode is the manifest's. -/

theorem bytecode_manifest : Export.Main.bytecodeHex = Export.Main.Manifest.bytecodeHex := rfl

/-- info: 'SolarTV.Milestone.W1.Sanity.stage0_overflow' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms stage0_overflow

/-- info: 'SolarTV.Milestone.W1.Sanity.evm4_overflow' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms evm4_overflow

end SolarTV.Milestone.W1.Sanity

