import SolarTV.MIR.Transforms.LowerAddressCall
import SolarTV.State.MemoryLaws

/-! Returndata materialization after a message. Lowering rounds the allocation
in word arithmetic, writes the length word, then copies the entire volatile
buffer. The equivalence preserves the returned pointer, free pointer, active
memory and dirty padding, including an empty return buffer. The length bound
only keeps the rounding arithmetic in range. Allocation and memory errors on
either path remain observable. -/

namespace SolarTV.MIR.LowerReturndata
open MIR.State EvmRules SolarTV.Execution

def size (length : Word) : Word := ((length + 31) &&& ~~~(31 : Word)) + 32

private theorem roundDown (n : Nat) (bound : n < 2^256) :
    n &&& (2^256 - 32) = n / 32 * 32 := by
  have high : (n &&& (2^256 - 32)) / 32 = n / 32 := by
    change (n &&& (2^256 - 32)) / 2^5 = n / 2^5
    rw [Nat.and_div_two_pow]
    have mask : (2^256 - 32) / 2^5 = 2^251 - 1 := by decide
    rw [mask, Nat.and_two_pow_sub_one_of_lt_two_pow]
    omega
  have low : (n &&& (2^256 - 32)) % 32 = 0 := by
    change (n &&& (2^256 - 32)) % 2^5 = 0
    rw [Nat.and_mod_two_pow]
    have mask : (2^256 - 32) % 2^5 = 0 := by decide
    rw [mask, Nat.and_zero]
  omega

theorem roundedSize (n : Nat) (bound : n + 63 < 2^256) :
    size (BitVec.ofNat 256 n) = BitVec.ofNat 256 ((n+63)/32*32) := by
  have nBound : n < 2^256 := by omega
  have addBound : n+31 < 2^256 := by omega
  have mask : (~~~(31 : Word)).toNat = 2^256 - 32 := by decide
  apply BitVec.eq_of_toNat_eq
  simp only [size, BitVec.toNat_add, BitVec.toNat_and, mask, BitVec.toNat_ofNat,
    Nat.mod_eq_of_lt nBound]
  change (((n + 31) % 2^256 &&& (2^256 - 32)) + 32) % 2^256 =
    ((n+63)/32*32) % 2^256
  rw [Nat.mod_eq_of_lt addBound, roundDown _ addBound]
  have sizeEq : (n+31)/32*32+32 = (n+63)/32*32 := by omega
  rw [sizeEq]

theorem roundedMask (n : Nat) (bound : n + 63 < 2^256) :
    ((BitVec.ofNat 256 n + 63) &&& ~~~(31 : Word)) = BitVec.ofNat 256 ((n+63)/32*32) := by
  have mask : (~~~(31 : Word)).toNat = 2^256 - 32 := by decide
  apply BitVec.eq_of_toNat_eq
  simp only [BitVec.toNat_and, BitVec.toNat_add, mask, BitVec.toNat_ofNat]
  rw [Nat.mod_eq_of_lt (by omega : n < 2^256)]
  change ((n+63)%2^256) &&& (2^256-32) = ((n+63)/32*32)%2^256
  rw [Nat.mod_eq_of_lt bound, roundDown _ bound, Nat.mod_eq_of_lt (by omega)]

theorem readWhole (bytes : List Byte) : readBytes bytes 0 bytes.length = bytes := by
  apply List.ext_getElem
  · simp [readBytes]
  · intro i hi hj
    simp [readBytes, List.getElem?_eq_getElem hj]

/-- The operation sequence emitted by lower-builtins, with SSA words inlined. -/
def lowered (cfg : Config) (frame : Frame) : Step := do
  let length := BitVec.ofNat 256 frame.returnData.length
  let rounded := size length
  if rounded.toNat < length.toNat then return .reverted (.panic 0x41) else
  match ← allocate frame rounded false false with
  | .returned (some pointer,next) =>
    match ← memoryOperation cfg next .store [pointer,length] with
    | .returned (none,next) =>
      match ← memoryOperation cfg next .returnDataCopy [pointer+32,0,length] with
      | .returned (none,next) => value next pointer
      | .reverted data => return .reverted data
      | .invalid => return .invalid
      | _ => .error "MIR returndata copy result mismatch"
    | .reverted data => return .reverted data
    | .invalid => return .invalid
    | _ => .error "MIR returndata header store result mismatch"
  | .reverted data => return .reverted data
  | .invalid => return .invalid
  | _ => .error "MIR returndata allocation result missing"

private theorem allocation_keeps_buffer (frame next : Frame) (size pointer : Word)
    (h : allocate frame size false false = .ok (.returned (some pointer,next))) :
    next.returnData = frame.returnData := by
  unfold allocate at h
  simp only [Except.bind, Bind.bind, Pure.pure, Except.pure] at h
  split at h
  · contradiction
  · split at h
    · contradiction
    · cases h
      rfl

theorem materialize (cfg : Config) (frame : Frame) (bound : frame.returnData.length + 63 < 2^256) :
    lowered cfg frame = returnDataBytes frame := by
  have nBound : frame.returnData.length < 2^256 := by omega
  have roundedBound : (frame.returnData.length+63)/32*32 < 2^256 := by omega
  have ordered : ¬ (frame.returnData.length+63)/32*32 < frame.returnData.length := by omega
  have noOverflow : ¬ 2^256 ≤ frame.returnData.length + 63 := by omega
  simp only [lowered, returnDataBytes, roundedSize _ bound, BitVec.toNat_ofNat,
    Nat.mod_eq_of_lt roundedBound, Nat.mod_eq_of_lt nBound, ordered, noOverflow,
    ite_false, Except.bind, Bind.bind, Pure.pure, Except.pure]
  cases h : allocate frame (BitVec.ofNat 256 ((frame.returnData.length+63)/32*32)) false false with
  | error _ => rfl
  | ok outcome =>
      cases outcome with
      | reverted _ | invalid => rfl
      | returned result =>
          obtain ⟨pointer,next⟩ := result
          cases pointer with
          | none => rfl
          | some pointer =>
              have buffer := allocation_keeps_buffer frame next _ pointer h
              simp only [memoryOperation, effect, value, wordBytes_length, BitVec.toNat_ofNat,
                Nat.mod_eq_of_lt nBound, Except.bind, Bind.bind, Pure.pure, Except.pure]
              cases expand next.memory pointer.toNat 32 with
              | error _ => rfl
              | ok memory =>
                simp [buffer, readWhole]
                cases copied : expand (writeBytes memory pointer.toNat
                  (wordBytes (BitVec.ofNat 256 frame.returnData.length)))
                  (pointer+32).toNat frame.returnData.length <;>
                  simp_all [BitVec.toNat_add]

abbrev Collected := Except String (Option (Outcome (Option Word × Option Word × Frame) × Nat))

def invoke (cfg : Config) (fuel : Nat)
    (prepared : Except String (Nat × List Word × Frame × Option Word)) : MessageStep := do
  let (opcode,args,frame,gas) ← prepared
  external fuel cfg frame opcode args gas

/-- Keep the real outcome of the child and its work. Materialization runs only
when the message returns to its caller. A failed child that returns data still
has a success word and reaches this continuation. -/
def collectMessage (collect : Frame → Step) (message : MessageStep) : Collected := do
  let some (result,work) ← message | return none
  match result with
  | .returned (success,frame) =>
    match ← collect frame with
    | .returned (pointer,frame) => return some (.returned (success,pointer,frame),work)
    | .reverted data => return some (.reverted data,work)
    | .invalid => return some (.invalid,work)
  | .reverted data => return some (.reverted data,work)
  | .invalid => return some (.invalid,work)

def invokeAndCollect (cfg : Config) (fuel : Nat)
    (prepared : Except String (Nat × List Word × Frame × Option Word))
    (collect : Frame → Step) : Collected :=
  collectMessage collect (invoke cfg fuel prepared)

theorem memoryObjectCall (cfg : Config) (fuel : Nat) (frame : Frame)
    (address input gas : Operand) (target requested : Word) (object : Scalar)
    (ha : read frame address = .ok target)
    (hi : readValue frame input = .ok (.scalar object))
    (hg : read frame gas = .ok requested)
    (bounded : ∀ opcode args preCall request success postCall work,
      LowerAddressCall.loweredMemoryObjectArgs cfg frame target requested object.word =
        .ok (opcode,args,preCall,request) →
      external fuel cfg preCall opcode args request =
        .ok (some (.returned (success,postCall),work)) →
      postCall.returnData.length + 63 < 2^256) :
    invokeAndCollect cfg fuel
      (messageArguments cfg frame (.address 0xf1 true false true false false) [address,input,gas])
      returnDataBytes =
    invokeAndCollect cfg fuel
      (LowerAddressCall.loweredMemoryObjectArgs cfg frame target requested object.word)
      (lowered cfg) := by
  rw [LowerAddressCall.memoryObjectArguments cfg frame address input gas target requested object ha hi hg]
  unfold invokeAndCollect invoke collectMessage
  cases prepare : LowerAddressCall.loweredMemoryObjectArgs cfg frame target requested object.word with
  | error _ => rfl
  | ok request =>
    obtain ⟨opcode,args,preCall,gasWord⟩ := request
    simp only [Except.bind, Bind.bind, Pure.pure, Except.pure]
    cases invoked : external fuel cfg preCall opcode args gasWord with
    | error _ => rfl
    | ok result =>
      cases result with
      | none => rfl
      | some completed =>
        obtain ⟨outcome,work⟩ := completed
        cases outcome with
        | invalid | reverted _ => rfl
        | returned payload =>
          obtain ⟨success,postCall⟩ := payload
          dsimp only
          rw [materialize cfg postCall (bounded _ _ _ _ _ _ _ prepare invoked)]

end SolarTV.MIR.LowerReturndata
