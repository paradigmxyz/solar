import SolarTV.EVMIR.State.ControlFlow
import SolarTV.EVMIR.Transforms.IndexedGuard

/-! Prove coverage of an indexed dispatch's range checks from physical entry.
The graph starts at PC zero and also at the possible misses and table targets
so the result applies after the initial dispatch. Stop at each certified
guard cut. The graph tracks known words in the live stack so it can resolve
branches and internal returns.
An unresolved jump or a reachable continuation into opaque data prevents an
entry certificate. A failed certificate leaves the local table proof intact. -/

namespace SolarTV.EVM.IndexedEntry
open EvmRules

structure Point where
  pc : Nat
  words : State.KnownStack := []
  deriving DecidableEq, BEq, ReflBEq, LawfulBEq

def Point.matches (point : Point) (frame : State.Frame) : Prop :=
  frame.pc = point.pc ∧ point.words.matches frame.stack

def isJump (opcode : Nat) : Bool :=
  opcode == 0x56 || opcode == 0x57

/-- Use the proved motion of a live stack, including across a branch. Branch
conditions and world state are unrestricted. -/
def successors (code : State.CodeChunks) (cfg : State.Config) (finalImage : Bool) (point : Point) :
    Option (List Point) := do
  if finalImage && code.size ≤ point.pc then return [] else
  let inst ← RelocatedReturn.decode code point.pc
  let words := (State.stackMotion cfg inst.opcode (code.immediate point.pc)).carry point.words
  if State.stopsControl inst.opcode then return []
  else if isJump inst.opcode then
    let target ← point.words.word 0
    -- The unchecked suffix may itself contain JUMPDEST, so a destination
    -- there must survive the projection and cause a failed certificate.
    let taken := if target.toNat ≥ code.size || (code.jumps 0).contains target.toNat
      then [Point.mk target.toNat words] else []
    return if inst.opcode = 0x57 then ⟨inst.next,words⟩ :: taken else taken
  else
    return [⟨inst.next,words⟩]

private def explore (code : State.CodeChunks) (cfg : State.Config) (finalImage : Bool)
    (cuts : List Nat) : Nat → List Point → List Point → Option (List Point)
  | _, [], seen => some seen
  | 0, _ :: _, _ => none
  | fuel+1, point :: rest, seen => do
    if seen.contains point then explore code cfg finalImage cuts fuel rest seen else
    let next ← if cuts.contains point.pc then some [] else successors code cfg finalImage point
    explore code cfg finalImage cuts fuel (next ++ rest) (point :: seen)

private theorem decoded (code : State.CodeChunks) (pc : Nat) (inst : RelocatedReturn.ByteOp)
    (found : RelocatedReturn.decode code pc = some inst) :
    ∃ byte, code.get? pc = some byte ∧ inst.opcode = byte.toNat ∧
      inst.next = pc + 1 + State.pushWidth inst.opcode ∧ inst.next ≤ code.size ∧
      inst.immediate = State.pushedWord inst.opcode (code.immediate pc) := by
  cases fetched : code.get? pc with
  | none => simp [RelocatedReturn.decode, fetched, Bind.bind, Option.bind] at found
  | some byte =>
    simp only [RelocatedReturn.decode, fetched, Bind.bind, Option.bind] at found
    split at found
    · contradiction
    · cases found
      refine ⟨byte, rfl, rfl, rfl, Nat.le_of_not_gt ‹_›, ?_⟩
      unfold State.pushedWord State.pushWidth
      by_cases zero : byte.toNat = 0x5f
      · simp [zero, State.CodeChunks.immediate]
      · by_cases wide : 0x60 ≤ byte.toNat ∧ byte.toNat ≤ 0x7f
        · have positive : 0 < byte.toNat - 0x5f := by omega
          simp [zero, wide, positive]
        · simp [zero, wide]

private theorem immediate_with_data (code : State.CodeChunks) (data : List State.Byte)
    (pc width : Nat) (fits : pc + 1 + width ≤ code.size) :
    State.pushValue (code.bytes ++ data) pc width = code.immediate pc width := by
  unfold State.pushValue State.CodeChunks.immediate Execution.readBytes
  congr 1
  apply List.map_congr_left
  intro i within
  rw [State.CodeChunks.get_eq, List.getElem?_append_left]
  rw [State.CodeChunks.size_eq]
  have := List.mem_range.mp within
  omega

private theorem pushed_with_data (code : State.CodeChunks) (data : List State.Byte)
    (pc opcode : Nat) (fits : pc + 1 + State.pushWidth opcode ≤ code.size) :
    State.pushedWord opcode (State.pushValue (code.bytes ++ data) pc) =
      State.pushedWord opcode (code.immediate pc) := by
  unfold State.pushedWord
  by_cases zero : opcode = 0x5f
  · simp [zero]
  · by_cases wide : 0x60 ≤ opcode ∧ opcode ≤ 0x7f
    · have enough : pc + 1 + (opcode - 0x5f) ≤ code.size := by
        simpa [State.pushWidth, wide] using fits
      simp [zero, wide, immediate_with_data code data pc _ enough]
    · simp [zero, wide]

/-- Refine every successful interpreter step into an edge of the checked
physical graph. The frame's code may include any suffix after the decoded
prefix. Resuming a child invocation leaves the untouched stack suffix live. -/
theorem successor_of_resume (code : State.CodeChunks) (data : List State.Byte)
    (cfg : State.Config) (execute : State.ChildExecutor) (world afterWorld : State.Journal)
    (frame planned after : State.Frame) (before : Point) (signal : State.Step)
    (cost : Nat) (edges : List Point) (finalImage : Bool)
    (finalSize : finalImage = true → data = [])
    (jumpInputs : ∀ opcode, isJump opcode = true → 0 < (State.available cfg opcode).inputs)
    (same : frame.code = code.bytes ++ data) (matched : before.matches frame)
    (known : successors code cfg finalImage before = some edges)
    (action : State.instruction cfg world frame = .ok signal)
    (continued : signal.continuation = some planned)
    (advanced : State.advance cfg execute signal = .ok (.inr (cost,afterWorld,after))) :
    ∃ point ∈ edges, point.matches after := by
  obtain ⟨beforePc, beforeWords⟩ := matched
  by_cases atEnd : (finalImage && decide (code.size ≤ before.pc)) = true
  · obtain ⟨finish, bound⟩ := by simpa using atEnd
    have done : frame.code.length ≤ frame.pc := by
      simpa [same, beforePc, finalSize finish, State.CodeChunks.size_eq] using
        (show code.size ≤ before.pc from bound)
    simp only [State.instruction, State.instructionWithImage, ite_eq_left done] at action
    cases action
    contradiction
  cases found : RelocatedReturn.decode code before.pc with
  | none => simp [successors, atEnd, found, Bind.bind, Option.bind, Option.pure_def] at known
  | some inst =>
    obtain ⟨byte, fetched, opcode, next, inside, immediate⟩ := decoded code before.pc inst found
    have current : frame.code[frame.pc]?.getD 0 = byte := by
      rw [beforePc, same, List.getElem?_append_left, ← State.CodeChunks.get_eq, fetched]
      · rfl
      · rw [State.CodeChunks.size_eq]
        omega
    have described := State.instruction_control cfg world frame frame.code.length byte
      (State.pushValue frame.code frame.pc) (State.validJump frame.code) signal planned
      (by simpa only [State.instruction, current] using action) continued
    obtain ⟨afterPc, resumed⟩ :=
      State.advance_control cfg execute signal planned after cost afterWorld continued advanced
    rw [← opcode, same, beforePc] at described
    obtain ⟨effect, stackEffect⟩ := described
    have literals := pushed_with_data code data before.pc inst.opcode (next ▸ inside)
    have motion : (State.stackMotion cfg inst.opcode (code.immediate before.pc)).realizes
        frame.stack after.stack := by
      simpa only [State.stackMotion, literals] using stackEffect after.stack resumed
    have carried := State.StackMotion.carry_matches _ _ _ _ beforeWords motion
    unfold State.ControlEffect at effect
    simp only [beforePc] at effect
    rw [literals, ← immediate] at effect
    simp only [successors, atEnd, found, Bind.bind, Option.bind, Option.pure_def] at known
    cases pushed : inst.immediate with
    | some value =>
      have wide : inst.opcode = 0x5f ∨ (0x60 ≤ inst.opcode ∧ inst.opcode ≤ 0x7f) := by
        rw [pushed] at immediate
        unfold State.pushedWord at immediate
        split at immediate
        · exact Or.inl ‹_›
        · split at immediate
          · exact Or.inr ‹_›
          · contradiction
      have harmless : State.stopsControl inst.opcode = false := by
        simp [State.stopsControl]
        omega
      have follows : isJump inst.opcode = false := by simp [isJump]; omega
      simp only [pushed, Option.isSome_some, ite_true] at effect
      simp only [harmless, follows, Bool.false_eq_true, ite_false, Option.some.injEq] at known
      subst edges
      exact ⟨_, List.mem_cons_self, afterPc.trans (effect.trans next.symm), carried⟩
    | none =>
      simp only [pushed, Option.isSome_none, ite_false,
        Bool.false_eq_true] at effect
      by_cases jump : isJump inst.opcode = true
      · have jumpCodes : inst.opcode = 0x56 ∨ inst.opcode = 0x57 := by
          simpa [isJump] using jump
        have noWidth : State.pushWidth inst.opcode = 0 := by
          rcases jumpCodes with h | h <;> simp [h, State.pushWidth]
        have noStop : State.stopsControl inst.opcode = false := by
          rcases jumpCodes with h | h <;> simp [h, State.stopsControl]
        simp only [noStop, Bool.false_eq_true, jump, ite_false, ite_true] at known
        cases address : before.words.word 0 with
        | none => simp [address] at known
        | some target =>
          simp only [address, Option.some.injEq] at known
          subst edges
          have top := beforeWords 0 target address
          rw [ite_eq_left jumpCodes, List.getElem?_take_of_lt (jumpInputs _ jump),
            top] at effect
          simp only [Option.getD_some] at effect
          rcases effect with ⟨valid, taken⟩ | ⟨cond, missed⟩
          · have admitted : (decide (target.toNat ≥ code.size) ||
                  (code.jumps 0).contains target.toNat) = true := by
              by_cases internal : target.toNat < code.size
              · rw [State.CodeChunks.jump_eq_with_data code data target internal, valid]
                simp
              · simp [show target.toNat ≥ code.size by omega]
            refine ⟨⟨target.toNat,_⟩, ?_, afterPc.trans taken, carried⟩
            rw [ite_eq_left admitted]
            by_cases conditional : inst.opcode = 0x57
            · rw [ite_eq_left conditional]
              exact List.mem_cons_of_mem _ List.mem_cons_self
            · rw [ite_eq_right conditional]
              exact List.mem_cons_self
          · refine ⟨⟨inst.next,_⟩, ?_, ?_, carried⟩
            · rw [ite_eq_left cond]
              exact List.mem_cons_self
            · simpa [next, noWidth] using afterPc.trans missed
      · have notJump : ¬(inst.opcode = 0x56 ∨ inst.opcode = 0x57) := by
          simpa [isJump] using jump
        rw [ite_eq_right notJump] at effect
        have noWidth : State.pushWidth inst.opcode = 0 := by
          by_cases wide : 0x60 ≤ inst.opcode ∧ inst.opcode ≤ 0x7f
          · rw [pushed] at immediate
            unfold State.pushedWord at immediate
            split at immediate <;> simp at immediate
          · simp [State.pushWidth, wide]
        have notTaken : isJump inst.opcode = false := by simpa using jump
        simp only [effect.1, notTaken, Bool.false_eq_true, ite_false, Option.some.injEq] at known
        subst edges
        refine ⟨⟨inst.next,_⟩, List.mem_cons_self, ?_, carried⟩
        simpa [next, noWidth] using afterPc.trans effect.2

def Closed (code : State.CodeChunks) (cfg : State.Config) (finalImage : Bool) (cuts : List Nat) (roots seen : List Point)
    (tables : List Nat) : Bool :=
  roots.all seen.contains && !(seen.any fun p => tables.contains p.pc) &&
    seen.all fun p => cuts.contains p.pc ||
      (((successors code cfg finalImage p).map fun next => next.all seen.contains).getD false)

def candidates (code : State.CodeChunks) (cfg : State.Config) (finalImage : Bool) (cuts : List Nat) (roots : List Point) :
    Option (List Point) :=
  explore code cfg finalImage cuts (min 8192 (2*code.size+roots.length)) roots []

/-- A same-frame bytecode trace before any proved selector producer. Nested
execution is quantified through the child executor. Only the parent's resumed
frame becomes the next point on this trace. -/
inductive FramePrefix (cfg : State.Config) (image : List State.Byte) (cuts : List Nat)
    (roots : List Point) : State.Frame → Prop
  | root (frame : State.Frame) (point : Point) (found : point ∈ roots)
      (matched : point.matches frame) : FramePrefix cfg image cuts roots frame
  | step (frame planned after : State.Frame) (path : FramePrefix cfg image cuts roots frame)
      (uncut : frame.pc ∉ cuts) (same : frame.code = image)
      (world afterWorld : State.Journal) (execute : State.ChildExecutor)
      (signal : State.Step) (cost : Nat)
      (action : State.instruction cfg world frame = .ok signal)
      (continued : signal.continuation = some planned)
      (advanced : State.advance cfg execute signal = .ok (.inr (cost,afterWorld,after))) :
      FramePrefix cfg image cuts roots after

/-- A closed set contains every concrete continuation before a selector
producer. The theorem makes no assumptions about initial gas, stack values,
calldata, world state or responses to external calls. -/
theorem cannotBypass (code : State.CodeChunks) (cfg : State.Config) (data : List State.Byte)
    (finalImage : Bool) (cuts : List Nat) (roots seen : List Point) (tables : List Nat)
    (finalSize : finalImage = true → data = [])
    (checked : Closed code cfg finalImage cuts roots seen tables = true)
    (jumpInputs : ∀ opcode, isJump opcode = true → 0 < (State.available cfg opcode).inputs)
    (frame : State.Frame) (path : FramePrefix cfg (code.bytes ++ data) cuts roots frame) :
    frame.pc ∉ tables := by
  simp only [Closed, Bool.and_eq_true, Bool.not_eq_true', List.all_eq_true] at checked
  have contained : ∃ point ∈ seen, point.matches frame := by
    induction path with
    | root frame point found matched =>
      exact ⟨point, by simpa using checked.1.1 point found, matched⟩
    | step frame planned after path uncut same world afterWorld execute signal cost action
        continued advanced ih =>
      obtain ⟨point, found, matched⟩ := ih
      have openCut : point.pc ∉ cuts := matched.1 ▸ uncut
      have closed := checked.2 point found
      simp only [List.contains_eq_mem, openCut, decide_false, Bool.false_or] at closed
      cases decoded : successors code cfg finalImage point with
      | none => simp [decoded] at closed
      | some edges =>
        simp only [decoded, Option.map_some, Option.getD_some, List.all_eq_true] at closed
        obtain ⟨next, edge, follows⟩ := successor_of_resume code data cfg execute world
          afterWorld frame planned after point signal cost edges finalImage finalSize
          jumpInputs same matched decoded
          action continued advanced
        exact ⟨next, by simpa using closed next edge, follows⟩
  obtain ⟨point, found, matched⟩ := contained
  intro inTables
  simp at checked
  exact checked.1.2 point found (matched.1 ▸ inTables)

end SolarTV.EVM.IndexedEntry
