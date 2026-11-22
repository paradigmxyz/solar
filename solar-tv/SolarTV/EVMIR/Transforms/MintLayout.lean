import SolarTV.EVMIR.State.BudgetedPath
import SolarTV.EVMIR.State.Observe

/-! The ERC20 mint layout certificate uses these shared data effects when
checking each emitted instruction. Calldata is read from the real environment;
current and original storage, access warmth and refunds come from the journal. -/
namespace SolarTV.EVM.MintLayout
open EvmRules SolarTV.Execution State State.BudgetedPath

def frame (code : State.CodeChunks) (env : World.Environment) (pc : Nat)
    (stack : List Word) (memory : List Byte) : Frame :=
  ⟨env,code.bytes,pc,stack,memory,[],0⟩

def expandTo (memory : List Byte) (size : Nat) : List Byte :=
  memory ++ List.replicate (size-memory.length) 0

def storedWord (memory : List Byte) (size offset : Nat) (value : Word) : List Byte :=
  writeBytes (expandTo memory size) offset (wordBytes value)

def readSlot (journal : Journal) (owner : Address) (slot : Word) : Word :=
  slotsRead (account journal owner).storage slot

def loadPrice (cfg : Config) (journal : Journal) (owner : Address) (slot : Word) : Nat :=
  Storage.readPrice cfg.fork (journal.warmSlots.contains (owner,slot))

def storePrice (cfg : Config) (journal : Journal) (owner : Address) (slot value : Word) :
    Storage.Price :=
  Storage.writePrice cfg.fork (journal.warmSlots.contains (owner,slot))
    (slotsRead ((journal.original.find? (fun x => x.address == owner)).getD {address := owner}).storage slot)
    (readSlot journal owner slot) value

def storeSlot (cfg : Config) (journal : Journal) (owner : Address) (slot value : Word) : Journal :=
  let price := storePrice cfg journal owner slot value
  let old := account journal owner
  let warmed := warmSlot journal owner slot
  putAccount {warmed with refund := warmed.refund + price.refund}
    {old with storage := slotsWrite old.storage slot value}

def log (journal : Journal) (owner : Address) (topics : List Word) (data : List Byte) : Journal :=
  {journal with logs := journal.logs ++ [⟨owner,topics,data⟩]}

theorem opcodeAt (cfg : Config) (world : Journal) (code : CodeChunks) (before : Frame)
    (balance : Nat) (opcode : Byte) (effect : Action)
    (image : before.code = code.bytes) (found : code.get? before.pc = some opcode)
    (checked : instructionWithImage cfg world {before with gas := balance}
      code.size opcode (code.immediate before.pc) (fun target => (code.jumps 0).contains target.toNat) =
        effect) :
    instruction cfg world {before with gas := balance} = effect := by
  have frame : ({before with gas := balance, code := code.bytes} : Frame) =
      {before with gas := balance} := by cases before; simp_all
  simpa only [frame, found, Option.getD_some] using
    (code.instruction_eq cfg world {before with gas := balance}).trans (by
      simpa only [frame, found, Option.getD_some] using checked)

theorem storeInstruction (cfg : Config) (journal : Journal) (code : CodeChunks)
    (env : World.Environment) (pc : Nat) (slot value : Word) (stack : List Word)
    (memory : List Byte) (balance : Nat) (fork : cfg.fork = .cancun)
    (schema : available cfg 85 = ⟨true,5000,0,2,0⟩) (writable : env.isStatic = false)
    (inside : pc < code.size) (height : stack.length ≤ 1024) :
    let price := storePrice cfg journal env.address slot value
    let result := Step.next (storeSlot cfg journal env.address slot value)
      {frame code env (pc+1) stack memory with gas := balance-price.gas}
    instructionWithImage cfg journal {frame code env pc (slot :: value :: stack) memory with gas := balance}
      code.size 85 (code.immediate pc) (fun target => (code.jumps 0).contains target.toNat) =
        if balance < max 2301 price.gas then stepExhausted journal else .ok result := by
  dsimp
  have check :
      instructionWithImage cfg journal {frame code env pc (slot :: value :: stack) memory with gas := balance}
        code.size 85 (code.immediate pc) (fun target => (code.jumps 0).contains target.toNat) =
          if balance ≤ 2300 then stepExhausted journal else
          if balance < (storePrice cfg journal env.address slot value).gas then stepExhausted journal
          else .ok (.next (storeSlot cfg journal env.address slot value)
            {frame code env (pc+1) stack memory with
              gas := balance-(storePrice cfg journal env.address slot value).gas}) := by
    have operands : ¬ stack.length + 1 + 1 < 2 := by omega
    simp [instructionWithImage,frame,schema,fork,Nat.not_le_of_gt inside,
      Nat.not_lt_of_ge height,Fork.atLeast,Fork.ordinal,pureOp,writable,
      operands,storeSlot,storePrice,readSlot,next]
  dsimp at check
  rw [check]
  by_cases sentry : balance ≤ 2300 <;>
    by_cases low : balance < (storePrice cfg journal env.address slot value).gas <;>
    have admission : (balance < max 2301 (storePrice cfg journal env.address slot value).gas) ↔
      (balance ≤ 2300 ∨ balance < (storePrice cfg journal env.address slot value).gas) := by omega
  all_goals simp only [admission,sentry,low,or_true,or_false,ite_true,ite_false]

/-- Low gas is observable. A refinement may turn an original OOG into the
funded result, and may retain more gas on every funded original execution. -/
theorem refines (world : Journal) (old new : List Debit) (exit : Exit) (gas : Nat)
    (admission : required new ≤ required old) (debit : charged new ≤ charged old)
    (paid : required old ≤ gas) :
    let before := settle world (expected old exit world gas)
    let after := settle world (expected new exit world gas)
    after.stop = before.stop ∧ after.data = before.data ∧ after.world = before.world ∧
      before.gas ≤ after.gas := by
  have paidNew : ¬ gas < required new := by omega
  have paidOld : ¬ gas < required old := by omega
  simp only [expected,paidOld,paidNew,ite_false,settle,Exit.result]
  split
  · simp
    omega
  · simp only [true_and]
    split <;> omega

def GasRefines (before after : Run) : Prop :=
  match before,after with
  | .ok a,.ok b => a.stop = .outOfGas ∨
    (b.stop = a.stop ∧ b.data = a.data ∧ b.world = a.world ∧ a.gas ≤ b.gas)
  | _,_ => False

theorem expected_refines (world : Journal) (old new : List Debit) (exit : Exit) (gas : Nat)
    (admission : required new ≤ required old) (debit : charged new ≤ charged old) :
    GasRefines (.ok (settle world (expected old exit world gas)))
      (.ok (settle world (expected new exit world gas))) := by
  by_cases low : gas < required old
  · simp [GasRefines,settle,expected,low]
  · exact Or.inr (refines world old new exit gas admission debit (by omega))

abbrev Plan := List Debit × Exit

structure Compared (before after : Run) (world : Journal) (gas : Nat) (old new : Plan) :
    Prop where
  prior : before = .ok (settle world (expected old.1 old.2 world gas))
  rewrite : after = .ok (settle world (expected new.1 new.2 world gas))
  admission : required new.1 ≤ required old.1
  debit : charged new.1 ≤ charged old.1
  terminal : old.2 = new.2

theorem Compared.refinement {before after : Run} {world : Journal} {gas : Nat} {old new : Plan}
    (proof : Compared before after world gas old new) : GasRefines before after := by
  rw [proof.prior,proof.rewrite,← proof.terminal]
  exact expected_refines world _ _ _ gas proof.admission proof.debit

end SolarTV.EVM.MintLayout
