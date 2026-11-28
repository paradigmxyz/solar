import SolarTV.MIR.State.LoopStep
import SolarTV.MIR.State.StoreTrace

/-! Two typed MIR loops write the same live memory word forever. One loop
takes an extra jump on its backedge. Check each iteration against the direct
interpreter, including the store event, then prove divergence on both sides. -/

set_option maxRecDepth 8192

namespace SolarTV.Examples.LoopSimulation
open EvmRules SolarTV.Execution MIR.State MIR.LoopSimulation

private def word (value : Word) : Operand := ⟨.constant ⟨256,value⟩,.word 256⟩
private def register (thunk : Bool) : Nat := if thunk then 1 else 0

private def carried (thunk : Bool) : Operand := ⟨.local (register thunk),.word 256⟩

private def incoming (thunk : Bool) : Phi :=
  ⟨register thunk,.word 256,[(0,word 7),(if thunk then 2 else 1,carried thunk)]⟩

private def write (thunk : Bool) : Instruction :=
  ⟨none,.word 0,some 0x52,.memory .store,[word 128,carried thunk]⟩

private def body (thunk : Bool) : Functions :=
  [(0,⟨[],[],[
    ⟨[],[],.jump 1⟩,
    ⟨[incoming thunk],[write thunk],.jump (if thunk then 2 else 1)⟩] ++
    (if thunk then [⟨[],[],.jump 1⟩] else []),none⟩)]

private def cfg : Config := ⟨.osaka,⟨0,0,0,0,0,0,[],false,0⟩,{},[0x52],false,128,none,[],{},[],false,none,false⟩

private def scratch (thunk : Bool) : Frame :=
  let memory := writeBytes (List.replicate 160 0) 128 (wordBytes 7)
  ⟨⟨[],[],[],[],[],0⟩,memory,[],[(register thunk,.scalar ⟨256,7⟩)],[],0,0,[]⟩

private def atBlock (thunk : Bool) (block previous : Nat) (entered : Bool := false) (next : Nat := 0) :
    Machine :=
  ⟨{function := 0,block := block,predecessor := some previous,entered := entered,next := next},
    [],scratch thunk⟩

private def entry (thunk : Bool) : Machine := atBlock thunk 1 (if thunk then 2 else 1)

private def primitive (thunk : Bool) : Loop Machine Store Run :=
  loopStep cfg (body thunk) 0 (storeAt (body thunk))

private def iteration (thunk : Bool) : Loop Machine Store Run :=
  fun _ => .again [(128,7)] (entry thunk)

private theorem enterHeader (thunk : Bool) :
    primitive thunk (entry thunk) =
      .again [] (atBlock thunk 1 (if thunk then 2 else 1) true) := by
  cases thunk <;>
    simp [primitive,loopStep,transition,body,write,incoming,carried,atBlock,entry,
      enter,readValue,required,put,storeAt,scratch,register,function,
      Bind.bind,Except.bind,Pure.pure,Except.pure,List.mapM,List.mapM.loop,ValueType.same,
      ↓reduceFits]

private theorem directPath : Segment (primitive false) (entry false)
    (iteration false (entry false)) := by
  have store : primitive false (atBlock false 1 1 true) = .again [(128,7)] (atBlock false 1 1 true 1) := rfl
  have back : primitive false (atBlock false 1 1 true 1) = .again [] (entry false) := rfl
  exact .more (enterHeader false) (.more store (.single back))

private theorem thunkPath : Segment (primitive true) (entry true)
    (iteration true (entry true)) := by
  have store : primitive true (atBlock true 1 2 true) = .again [(128,7)] (atBlock true 1 2 true 1) := rfl
  have toThunk : primitive true (atBlock true 1 2 true 1) = .again [] (atBlock true 2 1) := rfl
  have enterThunk : primitive true (atBlock true 2 1) = .again [] (atBlock true 2 1 true) := rfl
  have back : primitive true (atBlock true 2 1 true) = .again [] (entry true) := rfl
  exact .more (enterHeader true) (.more store (.more toThunk (.more enterThunk (.single back))))

private def cut (thunk : Bool) : Cutpoints (primitive thunk) (iteration thunk) where
  valid state := state = entry thunk
  path := by
    intro state equal
    subst state
    cases thunk
    · exact directPath
    · exact thunkPath
  nextValid := by
    intro state events next _ edge
    exact (Step.again.inj edge).2.symm

private def sim : Simulation (iteration true) (iteration false) where
  invariant _ _ := True
  step := by intros; exact ⟨rfl,trivial⟩

theorem orderedPrefixes (count : Nat) :
    ∃ l r, count ≤ l ∧ count ≤ r ∧
      observe (primitive true) l (entry true) =
        observe (primitive false) r (entry false) :=
  sim.concretePrefixes (cut true) (cut false) rfl rfl trivial count

theorem bothDiverge : diverges (primitive true) (entry true) ∧
    diverges (primitive false) (entry false) := by
  have always (n : Nat) : (observe (iteration false) n (entry false)).2 = none := by
    induction n with
    | zero => rfl
    | succ n ih => simpa [MIR.LoopSimulation.observe,iteration] using ih
  have right := ((cut false).diverges (state := entry false) rfl).mpr always
  exact ⟨(sim.concreteDivergence (cut true) (cut false) rfl rfl trivial).mpr right,right⟩

#print axioms orderedPrefixes
#print axioms bothDiverge

end SolarTV.Examples.LoopSimulation
