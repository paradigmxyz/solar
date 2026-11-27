import SolarTV.EVMIR.Transforms.EncodedTable

/-! Connect a dense switch's emitted range guard to its table. The branch
consumes its temporary words and preserves the selector and its stack suffix.
The out-of-range path reaches the JUMPI fallthrough on an unchanged stack.
On the taken path we charge the destination JUMPDEST before using the proved
table transfer. Everything starts at the decoded guard, below ABI dispatch. -/

namespace SolarTV.EVM.IndexedGuard
open EvmRules

structure Guard where
  limit : Word
  destination : Word
  miss : Word
  gas : Nat
  markerGas : Nat
  deriving DecidableEq

def read (cfg : State.Config) (code : State.CodeChunks) (start entry : Nat) : Option Guard := do
  if !(EncodedTable.decodedBoundaries cfg code && EncodedTable.instructionBoundary code start)
  then none else
  let dup ← RelocatedReturn.decode code start
  let limitPush ← RelocatedReturn.decode code dup.next
  let gt ← RelocatedReturn.decode code limitPush.next
  let addressPush ← RelocatedReturn.decode code gt.next
  let jumpi ← RelocatedReturn.decode code addressPush.next
  let limit ← limitPush.immediate
  let destination ← addressPush.immediate
  let marker ← RelocatedReturn.decode code destination.toNat
  let miss := BitVec.ofNat 256 jumpi.next
  if dup.opcode != 0x80 || gt.opcode != 0x11 || jumpi.opcode != 0x57 ||
      marker.opcode != 0x5b || marker.next != entry || miss == destination ||
      !(code.jumps 0).contains destination.toNat then none else
    let opcodes := [dup.opcode,limitPush.opcode,gt.opcode,addressPush.opcode,jumpi.opcode]
    let prices := opcodes.map (State.available cfg)
    let markerPrice := State.available cfg marker.opcode
    if !(prices.all (·.available)) || !markerPrice.available then none else
      return ⟨limit,destination,miss,(prices.map (·.base)).sum,markerPrice.base⟩

private def branch (guard : Guard) (stack : Stack) : Execution :=
  EVM.execute ⟨0,0,0,[],0⟩ [.dup 0,.push guard.limit,.gt]
    (.branch guard.destination guard.miss) stack

private theorem branch_result (guard : Guard) (selector : Word) (tail : Stack) :
    branch guard (selector :: tail) =
      if tail.length + 3 ≤ 1024 then
        .edge (if selector.ult guard.limit then guard.destination else guard.miss)
          (selector :: tail)
      else .failed .overflow := by
  unfold branch
  by_cases one : 1024 < tail.length + 1
  · have full : ¬tail.length + 3 ≤ 1024 := by omega
    simp [EVM.execute, stepChecked, List.length_cons, one, full]
  · by_cases two : 1024 < tail.length + 1 + 1
    · have full : ¬tail.length + 3 ≤ 1024 := by omega
      simp [EVM.execute, stepChecked, step, List.length_cons, one, two, full]
    · by_cases three : 1024 < tail.length + 1 + 1 + 1
      · have full : ¬tail.length + 3 ≤ 1024 := by omega
        simp [EVM.execute, stepChecked, step, List.length_cons, one, two, three, full]
      · have fit : tail.length + 3 ≤ 1024 := by omega
        cases comparing : selector.ult guard.limit <;>
          simp [EVM.execute, stepChecked, step, Evm.gt, finish, List.length_cons,
            one, two, three, fit, comparing]

inductive Exit where
  | miss (pc : Nat)
  | selected (pc : Nat)
  deriving DecidableEq

def run (cfg : State.Config) (code : State.CodeChunks) (start entry fuel jumps : Nat)
    (stack : Stack) (gas : Nat) : Option (Exit × Stack × Nat) := do
  let guard ← read cfg code start entry
  if gas < guard.gas then none else
  match branch guard stack with
  | .edge pc stack =>
    if pc = guard.destination then
      if gas < guard.gas + guard.markerGas then none else
      let (target,stack,remaining) ← EncodedTable.execute cfg code entry fuel jumps stack
        (gas-(guard.gas+guard.markerGas))
      return (.selected target,stack,remaining)
    else some (.miss pc.toNat,stack,gas-guard.gas)
  | _ => none

theorem misses (cfg : State.Config) (code : State.CodeChunks) (start entry fuel jumps : Nat)
    (guard : Guard) (found : read cfg code start entry = some guard)
    (distinct : guard.miss ≠ guard.destination) (selector : Word) (tail : Stack) (gas : Nat)
    (outside : guard.limit.toNat ≤ selector.toNat) :
    run cfg code start entry fuel jumps (selector :: tail) gas =
      if tail.length+3 ≤ 1024 && guard.gas ≤ gas
      then some (.miss guard.miss.toNat,selector::tail,gas-guard.gas) else none := by
  have comparison : selector.ult guard.limit = false := by
    simp [BitVec.ult, Nat.not_lt_of_ge outside]
  by_cases enough : guard.gas ≤ gas
  · have funded : ¬gas < guard.gas := by omega
    by_cases fits : tail.length+3 ≤ 1024 <;>
      simp [run, found, branch_result, comparison, distinct, enough, funded, fits,
        Bind.bind, Option.bind]
  · have short : gas < guard.gas := by omega
    simp [run, found, short, enough, Bind.bind, Option.bind]

/-- Starting at the guard discharges the table range on the selected path.
The bound comes from its encoded PUSH and covers arbitrary selector words. -/
theorem selects (cfg : State.Config) (code : State.CodeChunks) (start entry fuel jumps : Nat)
    (guard : Guard) (found : read cfg code start entry = some guard)
    (targets : List Nat) (rows : List (Nat × Nat))
    (valid : EncodedTable.check cfg code entry fuel jumps targets rows = true)
    (count : guard.limit.toNat ≤ targets.length)
    (selector : Word) (tail : Stack) (gas : Nat)
    (inside : selector.toNat < guard.limit.toNat) :
    ∃ pc peak cost, IndexedJump.target targets selector = some pc ∧
      rows[selector.toNat]? = some (peak,cost) ∧
      run cfg code start entry fuel jumps (selector::tail) gas =
        if max 3 peak + tail.length ≤ 1024 && guard.gas + guard.markerGas + cost ≤ gas
        then some (.selected pc,tail,gas-(guard.gas+guard.markerGas+cost)) else none := by
  have comparison : selector.ult guard.limit = true := by simp [BitVec.ult, inside]
  have inTable := Nat.lt_of_lt_of_le inside count
  have rowCount : targets.length = rows.length := by
    simp only [EncodedTable.check, Bool.and_eq_true, beq_iff_eq] at valid
    exact valid.1.1.1.2
  have inRows : selector.toNat < rows.length := rowCount ▸ inTable
  let bound := rows[selector.toNat]'inRows
  let pc := targets[selector.toNat]'inTable
  have logical : IndexedJump.target targets selector = some pc := by
    simp [IndexedJump.target, inTable, pc]
  have row : rows[selector.toNat]? = some (bound.1,bound.2) := by
    simp [inRows, bound]
  refine ⟨pc,bound.1,bound.2,logical,row,?_⟩
  have transfer := EncodedTable.correspondence cfg code entry fuel jumps targets rows valid
    selector tail (gas-(guard.gas+guard.markerGas)) pc bound.1 bound.2 logical row
  by_cases fit : tail.length+3 ≤ 1024
  · by_cases funded : guard.gas + guard.markerGas ≤ gas
    · have prefixFits : ¬gas < guard.gas := by omega
      have markerFits : ¬gas < guard.gas + guard.markerGas := by omega
      have room : max 3 bound.1 + tail.length ≤ 1024 ↔ bound.1 + tail.length ≤ 1024 := by omega
      have funds : guard.gas + guard.markerGas + bound.2 ≤ gas ↔
          bound.2 ≤ gas-(guard.gas+guard.markerGas) := by omega
      by_cases tableFits : bound.1 + tail.length ≤ 1024 <;>
        by_cases tablePaid : bound.2 ≤ gas-(guard.gas+guard.markerGas) <;>
        simp [run, found, prefixFits, branch_result, fit, comparison, markerFits, transfer,
          room, funds, tableFits, tablePaid, Nat.sub_sub, Bind.bind, Option.bind]
    · have notEnough : ¬guard.gas + guard.markerGas + bound.2 ≤ gas := by omega
      have short : gas < guard.gas + guard.markerGas := by omega
      by_cases low : gas < guard.gas <;>
        simp [run, found, branch_result, comparison, fit, short, low, notEnough,
          Bind.bind, Option.bind]
  · have full : ¬max 3 bound.1 + tail.length ≤ 1024 := by omega
    simp [run, found, branch_result, fit, full, Bind.bind, Option.bind]

/-- A selector at the smaller bound distinguishes the two guards on a funded
one-word stack at the same decoded cut. -/
def mismatch (cfg : State.Config) (code : State.CodeChunks) (start entry : Nat)
    (original : Word) : Option (Nat × Nat × Nat × Nat) := do
  let guard ← read cfg code start entry
  if original == guard.limit then none else
  let selector := BitVec.ofNat 256 (min original.toNat guard.limit.toNat)
  let .edge expected [_] := branch {guard with limit := original} [selector] | none
  let .edge actual [_] := branch guard [selector] | none
  if expected == actual then none else
    some (selector.toNat,expected.toNat,actual.toNat,guard.gas)

end SolarTV.EVM.IndexedGuard
