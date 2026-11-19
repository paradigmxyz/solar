import SolarTV.EVMIR.State.Messages

/-! Exact pricing from the entry of a scheduled zero-value CALL path. The
importer checks operands and branches before projecting the trace onto gas and
active memory. The checked reference below describes Morpho's transferFrom
path at this boundary. The entry gas is quantified, including insufficient
budgets. The no-code edge is an exit from the checked prefix. -/

namespace SolarTV.EVM.CallBudget
open EvmRules SolarTV.Execution

inductive Address where
  | fixed (offset : Nat)
  | heap (offset : Nat)
  deriving DecidableEq

def Address.at (fmp : Nat) : Address → Nat
  | .fixed offset => offset
  | .heap offset => fmp + offset

structure Access where
  base : Nat
  address : Address
  size : Nat
  deriving DecidableEq

structure Trace where
  guardBase : Nat
  probeCold : Nat
  probeWarm : Nat
  accesses : List Access
  untilSample : Nat
  afterSample : Nat
  callWarm : Nat
  deriving DecidableEq

structure Entry where
  gas : Nat
  fmp : Nat
  words : Nat
  warm : Bool
  hasCode : Bool

def Entry.ofFrame (world : Journal) (frame : State.Frame) (token : SolarTV.Execution.Address) :
    Entry :=
  ⟨frame.gas, (readWord frame.memory 64).toNat, memoryWords frame.memory,
    world.warmAddresses.contains token, !(account world token).code.isEmpty⟩

inductive Outcome where
  | outOfGas
  | noCode (gas : Nat)
  | call (sample available child retained activeWords : Nat)
  deriving DecidableEq

def active (current offset size : Nat) : Nat :=
  if size = 0 then current else max current ((offset + size + 31) / 32)

def peak (fmp : Nat) (accesses : List Access) (words : Nat) : Nat :=
  accesses.foldl (fun current op => active current (op.address.at fmp) op.size) words

def charged (gas price : Nat) : Option Nat :=
  if price ≤ gas then some (gas-price) else none

/-- Fixed charges and memory growth are nonnegative. Combining adjacent charges
preserves failure at every starting budget, including exact exhaustion. -/
theorem charged_add (gas first second : Nat) :
    (charged gas first).bind (fun left => charged left second) =
      charged gas (first+second) := by
  unfold charged
  by_cases h : first ≤ gas
  · simp only [h, ite_true, Option.bind_some]
    by_cases enough : second ≤ gas-first
    · have total : first+second ≤ gas := by omega
      simp [enough,total,Nat.sub_sub]
    · have total : ¬first+second ≤ gas := by omega
      simp [enough,total]
  · have total : ¬first+second ≤ gas := by omega
    simp [h,total]

private theorem memoryCost_mono {a b : Nat} (h : a ≤ b) :
    State.memoryCost a ≤ State.memoryCost b := by
  unfold State.memoryCost
  exact Nat.add_le_add (Nat.mul_le_mul_left _ h)
    (Nat.div_le_div_right (Nat.mul_le_mul h h))

def stepAccess (fmp gas words : Nat) (op : Access) : Option (Nat × Nat) :=
  let next := active words (op.address.at fmp) op.size
  (charged gas (op.base + (State.memoryCost next - State.memoryCost words))).map
    fun remaining => (remaining,next)

def runAccess (fmp : Nat) (ops : List Access) (gas words : Nat) : Option (Nat × Nat) :=
  match ops with
  | [] => some (gas,words)
  | op :: rest =>
    (stepAccess fmp gas words op).bind (fun (remaining,next) =>
      runAccess fmp rest remaining next)

def bases (accesses : List Access) : Nat := (accesses.map (·.base)).sum

private theorem active_ge (words offset size : Nat) : words ≤ active words offset size := by
  unfold active
  split
  · omega
  · exact Nat.le_max_left _ _

private theorem peak_ge (fmp : Nat) (ops : List Access) (words : Nat) :
    words ≤ peak fmp ops words := by
  induction ops generalizing words with
  | nil => exact Nat.le_refl _
  | cons op rest ih =>
    exact Nat.le_trans (active_ge words _ _) (ih _)

theorem runAccess_price (fmp : Nat) (ops : List Access) (gas words : Nat) :
    runAccess fmp ops gas words =
      (charged gas (bases ops + (State.memoryCost (peak fmp ops words) - State.memoryCost words))).map
        (fun remaining => (remaining,peak fmp ops words)) := by
  induction ops generalizing gas words with
  | nil => simp [runAccess, charged, bases, peak]
  | cons op rest ih =>
    let next := active words (op.address.at fmp) op.size
    let last := peak fmp rest next
    have first : State.memoryCost words ≤ State.memoryCost next :=
      memoryCost_mono (active_ge words (op.address.at fmp) op.size)
    have second : State.memoryCost next ≤ State.memoryCost last :=
      memoryCost_mono (peak_ge fmp rest next)
    have prices : op.base + (State.memoryCost next - State.memoryCost words) +
        (bases rest + (State.memoryCost last - State.memoryCost next)) =
        op.base + bases rest + (State.memoryCost last - State.memoryCost words) := by
      omega
    simp only [runAccess, stepAccess, Option.bind_map, Function.comp_def, ih,
      bases, List.map_cons, List.sum_cons, peak, List.foldl_cons]
    change ((charged gas (op.base + (State.memoryCost next - State.memoryCost words))).bind
      fun left => (charged left (bases rest + (State.memoryCost last - State.memoryCost next))).map
        (fun remaining => (remaining,last))) =
      (charged gas (op.base + bases rest + (State.memoryCost last - State.memoryCost words))).map
        (fun remaining => (remaining,last))
    have bindMap (x : Option Nat) (f : Nat → Option Nat) (g : Nat → Nat × Nat) :
        (x.bind (fun n => (f n).map g)) = (x.bind f).map g := by
      cases x <;> rfl
    rw [bindMap, charged_add, prices]

def probe (trace : Trace) (warm : Bool) : Nat :=
  if warm then trace.probeWarm else trace.probeCold

/-- Cancun CALL receives the explicit sampled word. Any instructions between
GAS and CALL are charged before CALL's own warm-access and memory charges. -/
def run (trace : Trace) (entry : Entry) : Outcome :=
  match charged entry.gas (trace.guardBase + probe trace entry.warm) with
  | none => .outOfGas
  | some left =>
    if !entry.hasCode then .noCode left else
    match runAccess entry.fmp trace.accesses left entry.words with
    | none => .outOfGas
    | some (left,words) =>
      match charged left trace.untilSample with
      | none => .outOfGas
      | some sample =>
        let need := active words (entry.fmp + 32) 100
        match charged sample (trace.afterSample + trace.callWarm +
            (State.memoryCost need - State.memoryCost words)) with
        | none => .outOfGas
        | some available =>
          match Calls.forwardGas .cancun available sample false with
          | none => .outOfGas
          | some budget => .call sample available budget.supplied (available-budget.charged) need

/-- Reviewed schedule contract. Each MLOAD/MSTORE charges and expands before
the next one. The literal prices here describe the reference independently of
the compiler schema used to import the actual opcodes. -/
def referenceTrace (selectorExtra : Nat := 0) : Trace := ⟨20,2600,100,
  [⟨6,.fixed 64,32⟩, ⟨18,.fixed 64,32⟩, ⟨9,.heap 0,32⟩,
   ⟨18+selectorExtra,.heap 32,32⟩, ⟨18,.heap 36,32⟩, ⟨15,.heap 68,32⟩, ⟨21,.heap 100,32⟩],
  17,0,100⟩

def beforeCall (entry : Entry) (selectorExtra : Nat := 0) : Nat :=
  let active := max entry.words ((entry.fmp+163)/32)
  (142+selectorExtra) + (if entry.warm then 100 else 2600) +
    (State.memoryCost active - State.memoryCost entry.words)

/-- Gas needed to finish the guard or, for a token with code, execute GAS.
The no-code error handler has its own gas and never executes GAS. -/
def sampleCharge (entry : Entry) (selectorExtra : Nat := 0) : Nat :=
  if entry.hasCode then beforeCall entry selectorExtra
  else (referenceTrace selectorExtra).guardBase + probe (referenceTrace selectorExtra) entry.warm

def reference (entry : Entry) (selectorExtra : Nat := 0) : Outcome :=
  let guard := 20 + (if entry.warm then 100 else 2600)
  if entry.gas < guard then .outOfGas
  else if !entry.hasCode then .noCode (entry.gas-guard)
  else if entry.gas < beforeCall entry selectorExtra + 100 then .outOfGas
  else
    let sample := entry.gas-beforeCall entry selectorExtra
    let available := sample-100
    .call sample available (available-available/64) (available/64)
      (max entry.words ((entry.fmp+163)/32))

private theorem peak_reference (fmp words selectorExtra : Nat) (heap : 128 ≤ fmp) :
    peak fmp (referenceTrace selectorExtra).accesses words = max words ((fmp+163)/32) := by
  simp only [peak, referenceTrace, List.foldl_cons, List.foldl_nil, active, Address.at]
  simp only [show ¬ (32 : Nat) = 0 by decide, ite_false]
  omega

theorem forward_cap (sample available : Nat) (h : available ≤ sample) :
    Calls.forwardGas .cancun available sample false =
      some ⟨available-available/64,available-available/64⟩ := by
  have cap : available-available/64 ≤ sample := Nat.le_trans (Nat.sub_le ..) h
  simp [Calls.forwardGas, Fork.atLeast, Fork.ordinal, Nat.min_eq_right cap]

set_option maxRecDepth 4096 in
theorem exact (entry : Entry) (heap : 128 ≤ entry.fmp) (selectorExtra : Nat := 0) :
    run (referenceTrace selectorExtra) entry = reference entry selectorExtra := by
  let highest := max entry.words ((entry.fmp+163)/32)
  let growth := State.memoryCost highest - State.memoryCost entry.words
  let guard := 20 + (if entry.warm then 100 else 2600)
  have inputEq : active highest (entry.fmp+32) 100 = highest := by
    simp only [active, show ¬ (100 : Nat) = 0 by decide, ite_false]
    dsimp only [highest]
    omega
  have peakEq : peak entry.fmp (referenceTrace selectorExtra).accesses entry.words = highest :=
    peak_reference entry.fmp entry.words selectorExtra heap
  unfold run reference
  simp only [runAccess_price, peakEq]
  have ledger : bases (referenceTrace selectorExtra).accesses = 105+selectorExtra := by
    simp only [bases, referenceTrace, List.map_cons, List.map_nil, List.sum_cons, List.sum_nil]
    omega
  rw [ledger]
  change (match charged entry.gas guard with
    | none => Outcome.outOfGas
    | some left =>
      if !entry.hasCode then Outcome.noCode left else
      match (charged left (105+selectorExtra+growth)).map (fun g => (g,highest)) with
      | none => Outcome.outOfGas
      | some (left,words) =>
        match charged left 17 with
        | none => Outcome.outOfGas
        | some sample =>
          match charged sample (0+100 + (State.memoryCost (active words (entry.fmp+32) 100) - State.memoryCost words)) with
          | none => Outcome.outOfGas
          | some available =>
            match Calls.forwardGas .cancun available sample false with
            | none => Outcome.outOfGas
            | some budget => Outcome.call sample available budget.supplied (available-budget.charged) (active words (entry.fmp+32) 100)) =
    if entry.gas < guard then Outcome.outOfGas
    else if !entry.hasCode then Outcome.noCode (entry.gas-guard)
    else if entry.gas < beforeCall entry selectorExtra + 100 then Outcome.outOfGas
    else Outcome.call (entry.gas-beforeCall entry selectorExtra) (entry.gas-beforeCall entry selectorExtra-100)
      (entry.gas-beforeCall entry selectorExtra-100-(entry.gas-beforeCall entry selectorExtra-100)/64)
      ((entry.gas-beforeCall entry selectorExtra-100)/64) highest
  have total : beforeCall entry selectorExtra = guard + (105+selectorExtra+growth) + 17 := by
    simp only [beforeCall, guard, growth, highest]
    omega
  rw [total]
  by_cases entered : guard ≤ entry.gas
  · simp only [charged, entered, ite_true, Nat.not_lt.mpr entered]
    split
    · rfl
    · by_cases memory : 105+selectorExtra+growth ≤ entry.gas-guard
      · simp only [memory, ite_true, Option.map_some, inputEq]
        by_cases setup : 17 ≤ entry.gas-guard-(105+selectorExtra+growth)
        · simp only [setup, ite_true, Nat.sub_self, Nat.add_zero, Nat.zero_add]
          let sample := entry.gas-guard-(105+selectorExtra+growth)-17
          by_cases paid : 100 ≤ sample
          · have success : ¬ entry.gas < guard+(105+selectorExtra+growth)+17+100 := by omega
            have raw : sample = entry.gas-(guard+(105+selectorExtra+growth)+17) := by omega
            simp only [show 100 ≤ entry.gas-guard-(105+selectorExtra+growth)-17 from paid,
              ite_true, success, ← raw]
            rw [forward_cap _ _ (Nat.sub_le ..)]
            dsimp only
            have cap : (sample-100)/64 ≤ sample-100 := Nat.div_le_self ..
            congr 1
            omega
          · have failed : entry.gas < guard+(105+selectorExtra+growth)+17+100 := by omega
            simp [show ¬100 ≤ entry.gas-guard-(105+selectorExtra+growth)-17 from paid, failed]
        · have failed : entry.gas < guard+(105+selectorExtra+growth)+17+100 := by omega
          simp [setup, failed]
      · have failed : entry.gas < guard+(105+selectorExtra+growth)+17+100 := by omega
        simp [memory, failed]
  · simp [charged, entered, Nat.lt_of_not_ge entered]

/-- An earlier GAS instruction can carry a larger word through the encoder.
The passed word may differ from the scheduled sample, but both requests reach
the same EIP-150 cap. Elapsed includes the earlier GAS instruction's own fee. -/
theorem earlierRequest (entry : Entry) (selectorExtra sample available child retained words elapsed : Nat)
    (reaches : reference entry selectorExtra = .call sample available child retained words)
    (earlier : elapsed ≤ beforeCall entry selectorExtra) :
    Calls.forwardGas .cancun available (entry.gas-elapsed) false = some ⟨child,child⟩ := by
  by_cases guard : entry.gas < 20 + (if entry.warm then 100 else 2600)
  · simp [reference, guard] at reaches
  by_cases noCode : (!entry.hasCode) = true
  · simp [reference, guard, noCode] at reaches
  by_cases paid : entry.gas < beforeCall entry selectorExtra + 100
  · simp [reference, guard, noCode, paid] at reaches
  simp only [reference, guard, noCode, paid, ite_false] at reaches
  cases reaches
  apply forward_cap
  omega

theorem memoryAccess (memory : List Byte) (offset size : Nat) :
    State.memoryCharge memory offset size =
      State.memoryCost (active (memoryWords memory) offset size) -
        State.memoryCost (memoryWords memory) := rfl

/-- In Cancun the EXTCODESIZE guard makes the later zero-value CALL warm, even
if the token was cold on entry. No account-creation or stipend fee applies. -/
theorem warmCall (cfg : State.Config) (world : Journal) (target : SolarTV.Execution.Address)
    (input : List Byte) (words : Nat) (fork : cfg.fork = .cancun) :
    State.callBoundaryCharge cfg (warmAddress world target)
      ⟨.call,target,0,input⟩ 0xf1 0 words words = 100 := by
  have warmed : target ∈ (warmAddress world target).warmAddresses := by
    by_cases present : target ∈ world.warmAddresses <;> simp [warmAddress,present]
  simp [State.callBoundaryCharge, Calls.hasValue, fork, Fork.atLeast, Fork.ordinal, warmed]

end SolarTV.EVM.CallBudget
