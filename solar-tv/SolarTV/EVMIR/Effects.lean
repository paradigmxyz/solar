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
Adjacent static PUSH/JUMPI pairs also expose their taken edges with the complete
live stack. A shared context supplies both physical charges, including PUSH0;
neither logical label numbers nor byte offsets are substituted for those prices.
-/

namespace SolarTV.EVM.Effects
open EvmRules

structure Request where
  opcode : Nat
  arguments : List Word
  returnsWord : Bool
  deriving DecidableEq

inductive Exit where
  | continuation (stack : Stack)
  | jump (label : Nat) (stack : Stack)
  deriving DecidableEq

inductive Op where
  | scalar (operation : EVM.Op)
  | effect (opcode inputs outputs : Nat)
  | guard (label pushCharge jumpCharge : Nat)

inductive Tree where
  | fault
  | done (charge : Nat) (exit : Exit)
  | request (charge : Nat) (effect : Request) (next : Word → Tree)

/-- A static label PUSH has a temporary stack slot even on the untaken edge. -/
def guardStack (label : Nat) (stack : Stack) : Option Exit :=
  if stack.length + 1 > 1024 then none else
    match stack with
    | condition :: rest => some (if condition = 0 then .continuation rest else .jump label rest)
    | [] => none

/-- Separate debits retain PUSH failure, temporary overflow and JUMPI failure.
The context supplies physical PUSH and JUMPI charges; logical labels do not
pretend to be physical byte offsets, including the PUSH0 case. -/
def guardCharged (label pushCharge jumpCharge : Nat) (stack : Stack) (gas : Nat) : Option (Exit × Nat) :=
  if gas < pushCharge then none
  else if stack.length + 1 > 1024 then none
  else if gas - pushCharge < jumpCharge then none
  else (guardStack label stack).map (fun exit => (exit,gas - pushCharge - jumpCharge))

theorem guardCharged_eq (label pushCharge jumpCharge : Nat) (stack : Stack) (gas : Nat) :
    guardCharged label pushCharge jumpCharge stack gas =
      (guardStack label stack).bind (fun exit =>
        if gas < pushCharge + jumpCharge then none else some (exit,gas - (pushCharge + jumpCharge))) := by
  by_cases full : stack.length + 1 > 1024
  · simp [guardCharged, guardStack, full]
  · cases stack with
    | nil => simp [guardCharged, guardStack]
    | cons condition rest =>
      have bounded : ¬1024 < rest.length + 1 + 1 := by simpa using full
      by_cases push : gas < pushCharge
      · have total : gas < pushCharge + jumpCharge := by omega
        simp [guardCharged,guardStack,push,total]
      · by_cases jump : gas - pushCharge < jumpCharge
        · have total : gas < pushCharge + jumpCharge := by omega
          simp [guardCharged,guardStack,push,jump,total]
        · have total : ¬gas < pushCharge + jumpCharge := by omega
          simp [guardCharged,guardStack,bounded,push,jump,total,Nat.sub_add_eq]

def expand (ctx : Context) (fork : Fork) : List Op → Stack → Nat → Tree
  | [], stack, charge =>
    if stack.length > 1024 then .fault else .done charge (.continuation stack)
  | .scalar op :: ops, stack, charge =>
    match stepChecked ctx op stack with
    | .error _ => .fault
    | .ok next => expand ctx fork ops next (charge + Gas.pureCost op fork)
  | .effect opcode inputs outputs :: ops, stack, charge =>
    if stack.length < inputs || stack.length - inputs + outputs > 1024 || stack.length > 1024 then
      .fault
    else .request charge ⟨opcode,stack.take inputs,outputs == 1⟩ fun word =>
      expand ctx fork ops (if outputs == 1 then word :: stack.drop inputs else stack.drop inputs) 0
  | .guard label pushCharge jumpCharge :: ops, stack, charge =>
    match guardStack label stack with
    | none => .fault
    | some (.continuation next) => expand ctx fork ops next (charge + (pushCharge + jumpCharge))
    | some exit => .done (charge + (pushCharge + jumpCharge)) exit

@[simp↓] theorem expand_guard_cons (ctx : Context) (fork : Fork)
    (label pushCharge jumpCharge charge : Nat) (ops : List Op) (condition : Word) (rest : Stack) :
    expand ctx fork (.guard label pushCharge jumpCharge :: ops) (condition :: rest) charge =
      if rest.length + 1 + 1 > 1024 then .fault else
        if condition = 0 then expand ctx fork ops rest (charge + (pushCharge + jumpCharge))
        else .done (charge + (pushCharge + jumpCharge)) (.jump label rest) := by
  by_cases full : rest.length + 1 + 1 > 1024 <;>
    by_cases zero : condition = 0#256 <;>
      simp [expand,guardStack,full,zero]

/-- Syntactic interaction refinement retains every effect and terminal stack.
Each interval's scalar work may only become cheaper. -/
def Refines : Tree → Tree → Prop
  | .fault,.fault => True
  | .done newer stack,.done older original => newer ≤ older ∧ stack = original
  | .request newer effect next,.request older original rest =>
    newer ≤ older ∧ effect = original ∧ ∀ reply, Refines (next reply) (rest reply)
  | _,_ => False

@[simp] theorem refines_ite (condition : Prop) [Decidable condition] (a b c d : Tree) :
    Refines (if condition then a else b) (if condition then c else d) ↔
      if condition then Refines a c else Refines b d := by split <;> rfl

abbrev Handler (World : Type) := Request → World → Option (Word × Nat × World)

def execute (handler : Handler World) : Tree → Nat → World → Option (Exit × Nat × World)
  | .fault, _, _ => none
  | .done charge stack, gas, world =>
    if gas < charge then none else some (stack,gas - charge,world)
  | .request charge effect next, gas, world =>
    if gas < charge then none else do
      let (reply,dynamic,world) ← handler effect world
      if gas - charge < dynamic then none else execute handler (next reply) (gas - charge - dynamic) world

/-- Instruction-by-instruction execution; each scalar pays before it executes. -/
def charged (handler : Handler World) (ctx : Context) (fork : Fork) :
    List Op → Stack → Nat → World → Option (Exit × Nat × World)
  | [], stack, gas, world =>
    if stack.length > 1024 then none else some (.continuation stack,gas,world)
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
  | .guard label pushCharge jumpCharge :: ops, stack, gas, world => do
    let (exit,gas) ← guardCharged label pushCharge jumpCharge stack gas
    match exit with
    | .continuation next => charged handler ctx fork ops next gas world
    | exit => some (exit,gas,world)

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
    | guard label pushCharge jumpCharge =>
      simp only [expand, charged, guardCharged_eq]
      cases branch : guardStack label stack with
      | none => simp [execute]
      | some exit =>
        cases exit with
        | continuation next =>
          simp only [Option.bind_some]
          rw [ih]
          by_cases prior : gas < pending
          · have total : gas < pending + (pushCharge + jumpCharge) := by omega
            simp [prior,total]
          · by_cases enough : gas - pending < pushCharge + jumpCharge
            · have total : gas < pending + (pushCharge + jumpCharge) := by omega
              simp [prior,enough,total]
            · have total : ¬gas < pending + (pushCharge + jumpCharge) := by omega
              simp [prior,enough,total,Nat.sub_add_eq]
        | jump target next =>
          by_cases prior : gas < pending
          · have total : gas < pending + (pushCharge + jumpCharge) := by omega
            simp [execute, prior,total]
          · by_cases enough : gas - pending < pushCharge + jumpCharge
            · have total : gas < pending + (pushCharge + jumpCharge) := by omega
              simp [execute, prior,enough,total]
            · have total : ¬gas < pending + (pushCharge + jumpCharge) := by omega
              simp [execute, prior,enough,total,Nat.sub_add_eq]

/-- The generic handler is shared, not assumed successful or memory-disjoint.
It includes the entire effect charge, including memory expansion and log data. -/
theorem resource (handler : Handler World) (before after : Tree)
    (refines : Refines after before) (world final : World) (stack : Exit)
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

example : ¬ Refines (.done 4 (.continuation [])) (.done 3 (.continuation [])) := by simp [Refines]

example : charged (fun _ (_ : Nat) => none) ⟨0,0,0,[]⟩ .cancun
    [.scalar (.push 1)] [] 2 0 = none := by decide

example : execute (fun _ (world : Nat) => some (0,10,world+1))
    (.request 3 ⟨0xa4,[0,0,0,0,0,0],false⟩ (fun _ => .done 0 (.continuation []))) 12 0 = none := by decide

example : guardCharged 0 2 10 [1,9] 12 = some (.jump 0 [9],0) := by decide
example : guardCharged 7 3 10 [0,9] 13 = some (.continuation [9],0) := by decide
example : guardCharged 7 3 10 [1] 12 = none := by decide
example : charged (fun _ (_ : Nat) => none) ⟨0,0,0,[]⟩ .cancun
    [.guard 7 3 10,.scalar .pop] [1] 13 0 = some (.jump 7 [],0,0) := by decide

example (stack : Stack) (full : stack.length = 1024) (gas pushCharge jumpCharge : Nat) :
    guardCharged 7 pushCharge jumpCharge stack gas = none := by
  simp [guardCharged,full]

end SolarTV.EVM.Effects
