import SolarTV.MIR.State.Interpreter

namespace SolarTV.MIR.PhiThread
open State

private def readAtom (frame : Frame) : Atom → Except String Value
  | .constant val => .ok (.scalar val)
  | .argument index => required frame.arguments[index]? "missing MIR argument"
  | .local id => required ((frame.locals.find? (·.1 == id)).map Prod.snd) "undefined MIR local"
  | .undefined => .ok .undefined

private theorem checked_read (frame : Frame) (operand : Operand) (value : Value)
    (read : readValue frame operand = .ok value) : value.fits 34 operand.ty = true := by
  have eq : readValue frame operand = (do
    let v ← readAtom frame operand.atom
    if !v.fits 34 operand.ty then .error "MIR operand type mismatch" else return v) := by
    obtain ⟨atom,ty⟩ := operand
    cases atom <;> rfl
  rw [eq] at read
  generalize valueRead : readAtom frame operand.atom = result at read
  cases result with
  | error message => simp [Bind.bind, Except.bind] at read
  | ok val =>
    by_cases fits : val.fits 34 operand.ty = true
    · have same : val = value := by
        simpa [Bind.bind, Except.bind, fits, Pure.pure, Except.pure] using read
      simpa [same] using fits
    · simp [Bind.bind, Except.bind, fits] at read

/-- Resolve the condition on the incoming edge when the phi is read only by
the following branch. The lexical liveness check for the local is separate. -/
theorem read_guard (frame : Frame) (pred : Nat) (phi : Phi) (input : Operand)
    (selected : phi.incoming.filter (·.1 == pred) = [(pred,input)])
    (ty : phi.ty = .word 1) (inputTy : input.ty = .word 1) :
    (do let next ← enter (some pred) [phi] frame
        readValue next ⟨.local phi.result,.word 1⟩) =
    readValue frame input := by
  simp only [enter, List.mapM_cons, selected, ty, inputTy]
  cases result : readValue frame input with
  | error reason => simp [Bind.bind, Except.bind]
  | ok value =>
    have fits : value.fits 34 (.word 1) = true := inputTy ▸ checked_read frame input value result
    have sameTy : (ValueType.word 1).same (.word 1) = true := by simp [ValueType.same]
    simp [Bind.bind, Except.bind, Pure.pure, Except.pure, sameTy, readValue, put,
      required, fits]

theorem read_branch (frame : Frame) (pred : Nat) (phi : Phi) (input : Operand)
    (selected : phi.incoming.filter (·.1 == pred) = [(pred,input)])
    (ty : phi.ty = .word 1) (inputTy : input.ty = .word 1) :
    (do let next ← enter (some pred) [phi] frame
        readBits 1 next ⟨.local phi.result,.word 1⟩) =
    readBits 1 frame input := by
  unfold readBits
  rw [← read_guard frame pred phi input selected ty inputTy]
  cases enter (some pred) [phi] frame <;> rfl

theorem untouched_reads (registers : Nat → Value) (result : Nat) (value : Value)
    (reads : List Nat) (dead : result ∉ reads) :
    reads.map (fun id => if id = result then value else registers id) =
      reads.map registers := by
  apply List.map_congr_left
  intro id present
  have other : id ≠ result := fun eq => dead (eq ▸ present)
  simp [other]

end SolarTV.MIR.PhiThread
