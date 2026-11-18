import SolarTV.MIR.State.Ops

/-! Slice construction and projection only update SSA locals. These lemmas
justify the totality condition of cyclic DCE for a typed slice and defined
scalar reads. A slice can hold any pointer and length bits, including ranges
that would fail a later memory access. No pointee is read.
-/

namespace SolarTV.MIR.DeadSlice
open MIR.State EvmRules

theorem make (cfg : Config) (frame : Frame) (id location : Nat)
    (pointer length : Operand) (p n : Word)
    (hp : read frame pointer = .ok p) (hn : read frame length = .ok n) :
    let slice := Value.aggregate [.scalar (Scalar.ofWord 256 p),.scalar (Scalar.ofWord 256 n)]
    instruction cfg frame ⟨some id,.slice location,none,.makeSlice location,[pointer,length]⟩ =
      effect {frame with locals := put frame.locals id slice} := by
  simp [instruction,hp,hn,Except.bind,Bind.bind,assignValue,Value.fits,ValueType.fields,
    ValueType.same,Scalar.ofWord,ValueType.width,List.all,List.zip]

private theorem field (location index : Nat) (value : Value) (bound : index < 2)
    (typed : value.fits 34 (.slice location) = true) :
    ∃ projected, extractField (.slice location) value index = .ok projected ∧
      projected.fits 34 (if index = 0 then .pointer location else .word 256) = true := by
  have index_cases : index = 0 ∨ index = 1 := by omega
  cases value with
  | undefined =>
    exact ⟨.undefined, index_cases.elim (fun h => by subst index; rfl)
      (fun h => by subst index; rfl), by simp [Value.fits]⟩
  | scalar _ => simp [Value.fits,ValueType.width] at typed
  | aggregate fields =>
    cases fields with
    | nil => simp [Value.fits,ValueType.fields] at typed
    | cons ptr tail =>
      cases tail with
      | nil => simp [Value.fits,ValueType.fields] at typed
      | cons len rest =>
        cases rest with
        | nil =>
          simp [Value.fits,ValueType.fields] at typed
          rcases index_cases with h | h <;> subst index
          · refine ⟨ptr,rfl,?_⟩
            cases ptr <;> exact typed.1
          · refine ⟨len,rfl,?_⟩
            cases len <;> exact typed.2
        | cons _ _ => simp [Value.fits,ValueType.fields] at typed

theorem extract (cfg : Config) (frame : Frame) (id location index : Nat)
    (operand : Operand) (value : Value) (bound : index < 2)
    (readValue_ok : readValue frame operand = .ok value)
    (typed : value.fits 34 (.slice location) = true) :
    ∃ projected,
      instruction cfg frame ⟨some id,(if index = 0 then .pointer location else .word 256),
        none,.extractValue (.slice location) index,[operand]⟩ =
        effect {frame with locals := put frame.locals id projected} := by
  obtain ⟨projected, h, fits⟩ := field location index value bound typed
  exact ⟨projected,by simp [instruction,readValue_ok,h,Except.bind,Bind.bind,assignValue,fits]⟩

end SolarTV.MIR.DeadSlice
