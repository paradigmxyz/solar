import SolarTV.EVMIR.Prefix

/-! Stateful stack-prefix certificates without a bounded memory projection.

Scalar instructions use the existing checked stack semantics and fork prices.
Other admitted instructions request their actual consumed words. A handler may
fail, or return a word, a new world and its full dynamic charge. It cannot read
the private stack tail or remaining gas. Code/GAS observers, calls and gas-sensitive
storage writes are therefore excluded by the importer. Memory and logs may alias
arbitrarily. The same requests must occur in the same order.

Refinement permits no greater scalar charge between any two effects. Every
successful original execution then has the same stack/world with no less gas
after rewriting, for every handler and initial world. Faults discard private
state as frame exceptions do. This is a logical window certificate; opcode
handler correspondence and physical code placement remain importer boundaries.
-/

namespace SolarTV.EVM.Effects
open EvmRules

structure Request where
  opcode : Nat
  arguments : List Word
  returnsWord : Bool
  deriving DecidableEq

inductive Op where
  | scalar (operation : EVM.Op)
  | effect (opcode inputs outputs : Nat)

inductive Tree where
  | fault
  | done (charge : Nat) (stack : Stack)
  | request (charge : Nat) (effect : Request) (next : Word → Tree)

def expand (ctx : Context) (fork : Fork) : List Op → Stack → Nat → Tree
  | [], stack, charge =>
    if stack.length > 1024 then .fault else .done charge stack
  | .scalar op :: ops, stack, charge =>
    match stepChecked ctx op stack with
    | .error _ => .fault
    | .ok next => expand ctx fork ops next (charge + Gas.pureCost op fork)
  | .effect opcode inputs outputs :: ops, stack, charge =>
    if stack.length < inputs || stack.length - inputs + outputs > 1024 || stack.length > 1024 then
      .fault
    else .request charge ⟨opcode,stack.take inputs,outputs == 1⟩ fun word =>
      expand ctx fork ops (if outputs == 1 then word :: stack.drop inputs else stack.drop inputs) 0

/-- Syntactic interaction refinement retains every effect and terminal stack.
Each interval's scalar work may only become cheaper. -/
def Refines : Tree → Tree → Prop
  | .fault,.fault => True
  | .done newer stack,.done older original => newer ≤ older ∧ stack = original
  | .request newer effect next,.request older original rest =>
    newer ≤ older ∧ effect = original ∧ ∀ reply, Refines (next reply) (rest reply)
  | _,_ => False

abbrev Handler (World : Type) := Request → World → Option (Word × Nat × World)

def execute (handler : Handler World) : Tree → Nat → World → Option (Stack × Nat × World)
  | .fault, _, _ => none
  | .done charge stack, gas, world =>
    if gas < charge then none else some (stack,gas - charge,world)
  | .request charge effect next, gas, world =>
    if gas < charge then none else do
      let (reply,dynamic,world) ← handler effect world
      if gas - charge < dynamic then none else execute handler (next reply) (gas - charge - dynamic) world

/-- Instruction-by-instruction execution; each scalar pays before it executes. -/
def charged (handler : Handler World) (ctx : Context) (fork : Fork) :
    List Op → Stack → Nat → World → Option (Stack × Nat × World)
  | [], stack, gas, world =>
    if stack.length > 1024 then none else some (stack,gas,world)
  | .scalar op :: ops, stack, gas, world =>
    if gas < Gas.pureCost op fork then none else
      match stepChecked ctx op stack with
      | .error _ => none
      | .ok next => charged handler ctx fork ops next (gas - Gas.pureCost op fork) world
  | .effect opcode inputs outputs :: ops, stack, gas, world =>
    if stack.length < inputs || stack.length - inputs + outputs > 1024 || stack.length > 1024 then none
    else do
      let (reply,dynamic,world) ← handler ⟨opcode,stack.take inputs,outputs == 1⟩ world
      if gas < dynamic then none else
        charged handler ctx fork ops
          (if outputs == 1 then reply :: stack.drop inputs else stack.drop inputs)
          (gas - dynamic) world

/-- Accumulating unobservable scalar charges does not change execution, including
failure order, dynamic charges and the complete live world on success. -/
theorem expansion_correct (handler : Handler World) (ctx : Context) (fork : Fork)
    (ops : List Op) (stack : Stack) (pending gas : Nat) (world : World) :
    execute handler (expand ctx fork ops stack pending) gas world =
      if gas < pending then none else charged handler ctx fork ops stack (gas - pending) world := by
  induction ops generalizing stack pending gas world with
  | nil =>
    by_cases full : stack.length > 1024 <;> simp [expand, execute, charged, full]
  | cons op ops ih =>
    cases op with
    | scalar op =>
      simp only [expand]
      cases stepped : stepChecked ctx op stack with
      | error reason => simp [charged, stepped, execute]
      | ok next =>
        rw [ih]
        by_cases prior : gas < pending
        · have total : gas < pending + Gas.pureCost op fork := by omega
          simp [prior,total]
        · by_cases enough : gas - pending < Gas.pureCost op fork
          · have total : gas < pending + Gas.pureCost op fork := by omega
            simp [charged, prior,enough,total]
          · have total : ¬gas < pending + Gas.pureCost op fork := by omega
            simp [charged, stepped, prior,enough,total,Nat.sub_add_eq]
    | effect opcode inputs outputs =>
      simp only [expand, charged]
      split
      · simp [execute]
      · simp only [execute]
        split
        · rfl
        · cases handled : handler ⟨opcode,stack.take inputs,outputs == 1⟩ world with
          | none => rfl
          | some result =>
            obtain ⟨reply,dynamic,world'⟩ := result
            simp only [Bind.bind, Option.bind]
            split
            · rfl
            · simpa only [Nat.sub_zero, Nat.not_lt_zero, ite_false] using
                ih (if outputs == 1 then reply :: stack.drop inputs else stack.drop inputs)
                  0 (gas - pending - dynamic) world'

/-- The generic handler is shared, not assumed successful or memory-disjoint.
It includes the entire effect charge, including memory expansion and log data. -/
theorem resource (handler : Handler World) (before after : Tree)
    (refines : Refines after before) (world final : World) (stack : Stack)
    (gas remaining extra : Nat)
    (success : execute handler before gas world = some (stack,remaining,final)) :
    ∃ remaining', execute handler after (gas + extra) world = some (stack,remaining',final) ∧
      remaining ≤ remaining' := by
  induction before generalizing after world final stack gas remaining extra with
  | fault => simp [execute] at success
  | done charge original =>
    cases after with
    | fault => simp [Refines] at refines
    | request _ _ _ => simp [Refines] at refines
    | done newer next =>
      obtain ⟨cheaper,rfl⟩ := refines
      by_cases enough : gas < charge
      · simp [execute, enough] at success
      · have enough' : ¬gas + extra < newer := by omega
        simp [execute, enough] at success
        obtain ⟨rfl,rfl,rfl⟩ := success
        exact ⟨gas + extra - newer, by simp [execute, enough'], by omega⟩
  | request charge effect rest ih =>
    cases after with
    | fault => simp [Refines] at refines
    | done _ _ => simp [Refines] at refines
    | request newer request next =>
      obtain ⟨cheaper,rfl,children⟩ := refines
      by_cases enough : gas < charge
      · simp [execute, enough] at success
      · have enough' : ¬gas + extra < newer := by omega
        simp only [execute, enough, enough', ite_false] at success ⊢
        cases handled : handler request world with
        | none => simp [handled] at success
        | some result =>
          obtain ⟨reply,dynamic,world'⟩ := result
          simp only [handled, Bind.bind, Option.bind] at success ⊢
          by_cases dynamicEnough : gas - charge < dynamic
          · simp [dynamicEnough] at success
          · have dynamicEnough' : ¬gas + extra - newer < dynamic := by omega
            simp only [dynamicEnough, dynamicEnough', ite_false] at success ⊢
            have slack : gas + extra - newer - dynamic =
                (gas - charge - dynamic) + (extra + charge - newer) := by omega
            rw [slack]
            exact ih reply (next reply) (children reply) world' final stack
              _ remaining _ success

example : ¬ Refines (.done 4 []) (.done 3 []) := by simp [Refines]

example : charged (fun _ (_ : Nat) => none) ⟨0,0,0,[]⟩ .cancun
    [.scalar (.push 1)] [] 2 0 = none := by decide

example : execute (fun _ (world : Nat) => some (0,10,world+1))
    (.request 3 ⟨0xa4,[0,0,0,0,0,0],false⟩ (fun _ => .done 0 [])) 12 0 = none := by decide

end SolarTV.EVM.Effects
