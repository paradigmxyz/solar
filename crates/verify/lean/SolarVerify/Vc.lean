import SolarVerify.Smt
/-!
Verification conditions as solar-verify exports them: a script of declarations and assertions,
as the solver's assertion stack held them, and obligations that each ask the solver to show a
goal under a guard and the first assertions of the script. See docs/VERIFY_LEAN.md.
-/
namespace SolarVerify.Vc
open SolarVerify.Smt

/-- The version of the export format this library reads. -/
def formatVersion : Nat := 1

/-- The declarations and assertions in scope in the solver, in the order solar-verify sent them.
A `define-fun` is a declaration and an assertion that it equals its body at every argument. -/
structure Script where
  decls : List Decl
  asserts : List Term
  deriving Repr, Inhabited

/-- One query: the goal follows from the first `upto` assertions of the script and the guard.
solar-verify asks the solver whether `(and asserts guard (not goal))` is unsatisfiable. -/
structure Obligation where
  script : Script
  upto : Nat
  guard : Term
  goal : Term
  deriving Repr, Inhabited

/-- The obligation as one closed formula, `(=> (and a_0 ... a_{upto-1} guard) goal)`. -/
def Obligation.formula (o : Obligation) : Term :=
  .app .implies [.app .and (o.script.asserts.take o.upto ++ [o.guard]), o.goal]

/-- The obligation holds: its formula is well-sorted and true under every interpretation of the
declared symbols. Exactly when the query solar-verify sends is unsatisfiable. -/
def Obligation.Valid (o : Obligation) : Prop :=
  ∀ env : Env, Holds env o.script.decls o.formula

end SolarVerify.Vc
