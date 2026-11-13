import W1Proofs.Tactic
import W1Proofs.Axioms
import SolarTV.Reentrancy
import verify_vault.gen.Vault.Evm15

/-! The reentrancy lock of `crates/verify/examples/Vault.sol`, at its final EVM IR.

`Vault` keeps its lock `locked`, a `bool`, in the low byte of slot 3. While the lock is held, no
successful run of the final EVM IR changes storage, whatever the oracle: `deposit` and `withdraw`
revert at `require(!locked)`, and the getters write nothing. With `Reentrancy.Respects.frame`, the
token calls `deposit` and `withdraw` make while holding the lock leave `Vault`'s storage as it
was, which is what solar-verify assumes when it havocs nothing across a call made under a lock
that no entry point stays open under. -/

namespace SolarTV.Milestone.VerifyVault

open EvmRules Machine SolarTV.Export.Vault SolarTV.Milestone.W1 SolarTV.Reentrancy

/-- `locked` holds: the low byte of slot 3 is not zero. -/
def Locked (σ : Storage) : Prop := ¬ Evm.and (σ 3) 255 = 0

/-- What a run shows of the lock frame: it ends, and a committed end keeps the storage. -/
def Frames (call : Call) : Option (Halt × State) → Prop
  | some r => Halt.commits r.1 = true → r.2.storage = (State.initial call).storage
  | none => False

set_option maxHeartbeats 8000000 in
set_option maxRecDepth 20000 in
/-- Symbolic execution of the final EVM IR from a locked storage, on every call and oracle: each
path ends within 16 blocks, and each committed end keeps the storage. -/
theorem locked_run (o : Oracle) (call : Call) (locked : Locked (State.initial call).storage) :
    Frames call (EVM.Program.run o evm15 call 16) := by
  unfold Locked at locked
  dsimp only [State.initial, BitVec.reduceToNat] at locked
  obtain ⟨cd, hcd⟩ : ∃ cd, cd = calldataWord call := ⟨_, rfl⟩
  repeat (first | sym_eval | sym_cond | dsimp only [State.initial, Frames, Halt.commits])
  repeat' (split_cond <;>
    repeat (first | sym_eval | sym_cond | dsimp only [State.initial, Frames, Halt.commits]))
  all_goals first
    | exact fun _ => trivial
    | fail "locked_run: a committed run under the lock may change storage on this path"

/-- No successful run of the final EVM IR, under any oracle, changes storage while `locked`
holds. -/
theorem lock_frame : LockFrame (L := EVM.Program.layer) (p := evm15) Locked := by
  intro o call r locked ends commits
  have h := locked_run o call locked
  cases hrun : EVM.Program.run o evm15 call 16 with
  | none => rw [hrun] at h; exact h.elim
  | some r₀ =>
    have : r = r₀ := EVM.Program.layer.ends_unique ends ⟨16, hrun⟩
    subst this
    rw [hrun] at h
    exact h commits

/-- Under every oracle of a chain where only `Vault`'s own code writes its storage, at every
call depth: a call or creation the final EVM IR makes while `locked` holds leaves its storage as
it was. -/
theorem locked_call_keeps_storage {n : Nat} {o : Oracle}
    (respects : Respects EVM.Program.layer evm15 n o) {world : List Event} {σ : Storage}
    (handoff : Handoff world σ) (locked : Locked σ) : o.storageAfter world = σ :=
  respects.frame lock_frame handoff locked

/-- info: 'SolarTV.Milestone.VerifyVault.locked_call_keeps_storage' depends on axioms within [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#axioms_within locked_call_keeps_storage

end SolarTV.Milestone.VerifyVault
