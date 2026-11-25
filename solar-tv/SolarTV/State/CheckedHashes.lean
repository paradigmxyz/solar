import SolarTV.Keccak

/-! Reuse concrete Keccak reductions in an execution certificate. A hit carries
its kernel-checked digest. A miss computes the original hash. -/

namespace SolarTV.Keccak
open EvmRules

structure CheckedHash where
  input : List Byte
  output : Word
  checked : hash input = output

def withChecks (entries : List CheckedHash) (input : List Byte) : Word :=
  match entries with
  | [] => hash input
  | entry :: rest => if input == entry.input then entry.output else withChecks rest input

theorem withChecks_eq (entries : List CheckedHash) :
    withChecks entries = hash := by
  funext input
  induction entries with
  | nil => rfl
  | cons entry rest ih =>
    unfold withChecks
    split
    · rename_i equal
      rw [beq_iff_eq] at equal
      simpa [equal] using entry.checked.symm
    · exact ih

end SolarTV.Keccak
