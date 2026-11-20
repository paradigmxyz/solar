import SolarTV.MIR.State.Interpreter
import SolarTV.EVMIR.State.Reentry
import SolarTV.MIR.Transforms.LowerReturndata
import EvmRules.Lemmas

/-! The unchanged address getter can execute from MIR inside a reentrant
message. The physical entry supplies its own fixed instruction charges,
memory expansion and warm/cold storage charge. -/

namespace SolarTV.MIR.ReentrantOwner
open EvmRules SolarTV.Execution MIR.State

def loaded (env : World.Environment) (world : Journal) : Word :=
  slotsRead (account world env.address).storage 0

def owner (env : World.Environment) (world : Journal) : Word :=
  (loaded env world |>.setWidth 160).setWidth 256

def price (env : World.Environment) (world : Journal) : Nat :=
  SolarTV.Storage.readPrice .cancun (world.warmSlots.contains (env.address,0))

def ownerBody : Function :=
  ⟨[],[.word 160],
    [⟨[],[⟨some 1,.word 256,some 0x54,.storage false false,[⟨.constant ⟨256,0⟩,.word 256⟩]⟩,
        ⟨some 2,.word 160,none,.lowBits 160,[⟨.local 1,.word 256⟩]⟩],
      .returned [⟨.local 2,.word 160⟩]⟩],some "owner"⟩

def config (ext : EVM.State.Config) (env : World.Environment) : Config :=
  {fork := ext.fork, environment := env, context := ext.block, available := [0x54], entry := false}

def finishOwner (ext : EVM.State.Config) (id fee work : Nat) (body : Function)
    (world : Journal) (frame : EVM.State.Frame) : EVM.State.Run := do
  let result ← execute 4 (config ext frame.environment) [(id,body)] id
    {world, memory := [], arguments := []}
  match result.completion with
  | .returned [address] => return ⟨.success,wordBytes address,result.world,frame.gas-fee,work⟩
  | _ => .error "owner getter did not return an address"

theorem finish_eq (ext : EVM.State.Config) (id fee work : Nat)
    (world : Journal) (frame : EVM.State.Frame) :
    finishOwner ext id fee work ownerBody world frame =
      .ok ⟨.success,wordBytes (owner frame.environment world),
        warmSlot world frame.environment.address 0,frame.gas-fee,work⟩ := by
  let cfg := config ext frame.environment
  let table : Functions := [(id,ownerBody)]
  let start : Frame := {world,memory := [],arguments := []}
  let caught : Frame := {start with
    world := warmSlot world frame.environment.address 0
    locals := [(1,.scalar (Scalar.ofWord 256 (loaded frame.environment world)))]}
  let packed : Frame := {caught with
    locals := put caught.locals 2 (.scalar ⟨160,(loaded frame.environment world).setWidth 160⟩)}
  let atEntry : Machine := ⟨{function := id},[],start⟩
  let entered : Machine := ⟨{function := id,entered := true},[],start⟩
  let afterRead : Machine := ⟨{function := id,entered := true,next := 1},[],caught⟩
  let atReturn : Machine := ⟨{function := id,entered := true,next := 2},[],packed⟩
  have enter_eq : transition 3 cfg table atEntry = .ok (.next 0 entered) := by
    simp [atEntry, entered, table, transition, function, ownerBody,
      required, enter, Bind.bind, Except.bind, Pure.pure, Except.pure]
  have load_eq : transition 2 cfg table entered = .ok (.next 0 afterRead) := by
    simp [entered, afterRead, table, transition, function, ownerBody,
      config, cfg, start, caught, required, instruction, operation, storageOperation,
      State.read, readValue, Value.word, next, advance, loaded,
      value, effect, assign, assignValue, Value.fits, Scalar.ofWord, ValueType.width,
      put, Bind.bind, Except.bind, Pure.pure, Except.pure]
  have trunc_eq : transition 1 cfg table afterRead = .ok (.next 0 atReturn) := by
    simp [afterRead, atReturn, table, transition, function, ownerBody, caught, packed,
      required, instruction, operation, State.read, readValue, Value.word, next, advance,
      value, assign, assignValue, Value.fits, ValueType.width, effect, narrow,
      Scalar.ofWord, wordBits, put,
      Bind.bind, Except.bind, Pure.pure, Except.pure] <;> rfl
  have return_eq : transition 0 cfg table atReturn =
      .ok (.finished (finish (.returned [owner frame.environment world]) packed)) := by
    rw [transition]
    simp only [table, function, List.find?_cons, beq_self_eq_true,
      required, Option.map_some, Bind.bind, Except.bind, ownerBody]
    rfl
  have executed : run 4 cfg table atEntry =
      .ok (finish (.returned [owner frame.environment world]) packed) := by
    simp [run_next 3 enter_eq, run_next 2 load_eq, run_next 1 trunc_eq, run_finished 0 return_eq]
  have started : State.start table id start.arguments start = .ok start := by
    simp [State.start, table, function, required, typedWords, start, ownerBody,
      Bind.bind, Except.bind, Pure.pure, Except.pure]
  have execution : execute 4 cfg table id start =
      .ok (finish (.returned [owner frame.environment world]) packed) := by
    unfold execute
    rw [show cfg.callDepth = 128 from rfl, started]
    change conclude cfg start (run 4 cfg table atEntry) = _
    rw [executed]
    rfl
  unfold finishOwner
  rw [execution]
  rfl

theorem canon (word : Word) :
    Evm.shr 96 (Evm.shl 96 word) = (word.setWidth 160).setWidth 256 := by
  simp only [Evm.shl, Evm.shr, show (96 : Word).ult 256 = true from rfl, Bool.cond_true]
  bv_decide

theorem outputMemory (word : Word) :
    expand [] 0 32 = .ok (List.replicate 32 0) ∧
    writeBytes (List.replicate 32 0) 0 (wordBytes word) = wordBytes word ∧
    expand (wordBytes word) 0 32 = .ok (wordBytes word) ∧
    readBytes (wordBytes word) 0 32 = wordBytes word := by
  refine ⟨rfl, ?_, ?_, ?_⟩
  · simp [writeBytes, List.take_of_length_le]
  · simp [expand, requiredWords, memoryWords, wordBytes_length]
  · simpa only [wordBytes_length] using LowerReturndata.readWhole (wordBytes word)

end SolarTV.MIR.ReentrantOwner
