import SolarTV.EVMIR.Ops

/-! Exceptional stack behavior of the bounded EVM window machine.
These lemmas use symbolic tails at the full 1024-word boundary, following
EVMYulLean EVM/Semantics.lean:450-471. Terminal halts ignore leftover words;
an earlier underflow or overflow still terminates execution immediately. -/

namespace SolarTV.EVM
open EvmRules

theorem pop_underflows (ctx : Context) : stepChecked ctx .pop [] = .error .underflow := rfl

theorem dup_underflows (ctx : Context) (value : Word) :
    stepChecked ctx (.dup 1) [value] = .error .underflow := rfl

theorem swap_underflows (ctx : Context) (value : Word) :
    stepChecked ctx (.swap 0) [value] = .error .underflow := rfl

theorem push_at_limit (ctx : Context) (stack : Stack) (value : Word)
    (full : stack.length = 1024) :
    stepChecked ctx (.push value) stack = .error .overflow := by
  simp [stepChecked, step, full]

theorem dup_at_limit (ctx : Context) (value : Word) (rest : Stack)
    (full : rest.length = 1023) :
    stepChecked ctx (.dup 0) (value :: rest) = .error .overflow := by
  simp [stepChecked, step, full]

theorem swap_at_limit (ctx : Context) (a b : Word) (rest : Stack)
    (full : rest.length = 1022) :
    stepChecked ctx (.swap 0) (a :: b :: rest) = .ok (b :: a :: rest) := by
  simp [stepChecked, step, full]

theorem push_pop_cannot_recover (ctx : Context) (stack : Stack) (value : Word)
    (full : stack.length = 1024) :
    execute ctx [.push value, .pop] .stop stack = .failed .overflow := by
  simp [execute, push_at_limit ctx stack value full]

theorem dropping_full_push_pop_changes_outcome (ctx : Context) (stack : Stack) (value : Word)
    (full : stack.length = 1024) :
    execute ctx [.push value, .pop] .stop stack ≠ execute ctx [] .stop stack := by
  rw [push_pop_cannot_recover ctx stack value full]
  simp [execute, finish, full]

theorem full_stack_can_halt (stack : Stack) (full : stack.length = 1024) :
    finish .stop stack = .success := by simp [finish, full]

theorem label_push_can_overflow (stack : Stack) (full : stack.length = 1024) :
    finish (.jump 1) stack = .failed .overflow := by simp [finish, full]

theorem empty_return_needs_two_words (value : Word) :
    finish .returnEmpty [value] = .failed .underflow := rfl

theorem empty_return_discards_stack (offset : Word) (rest : Stack)
    (legal : rest.length ≤ 1022) :
    finish .returnEmpty (offset :: 0 :: rest) = .success := by
  simp [finish]
  omega

theorem untaken_branch_executes_stop (destination : Word) (rest : Stack)
    (legal : rest.length ≤ 1022) :
    finish (.branchStop destination) (0 :: rest) = .success := by
  have h1 : ¬ (rest.length + 1 > 1024) := by omega
  have h2 : ¬ (rest.length + 1 + 1 > 1024) := by omega
  simp [finish, h1, h2]

theorem taken_branch_leaves_the_block (destination condition : Word) (rest : Stack)
    (nonzero : condition ≠ 0) (legal : rest.length ≤ 1022) :
    finish (.branchStop destination) (condition :: rest) = .edge destination rest := by
  have h1 : ¬ (rest.length + 1 > 1024) := by omega
  have h2 : ¬ (rest.length + 1 + 1 > 1024) := by omega
  simp [finish, h1, h2]
  exact nonzero

end SolarTV.EVM
