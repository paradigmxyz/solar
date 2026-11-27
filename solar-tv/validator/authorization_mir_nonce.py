"""Check the actual MIR nonce prefix against the emitted store operands."""

from . import mir_state
from .callers import require

DEFINITIONS = """
open SolarTV.Execution MIR.State MIR.TransferFromABI MIR.TransferFromMarket

def suppliedNonce (env : World.Environment) : Word := readWord env.input 100

def nonceArena (env : World.Environment) : List Byte :=
  wordArena (nonceOwner env :: 7 :: (authWords env).drop 2)

theorem nonceArenaLoad (env : World.Environment) (frame : Frame) (index : Nat) (v : Word)
    (within : index+1 ≤ (authWords env).length)
    (cell : (authWords env)[index]? = some v) (active : frame.memory = authArena env) :
    operation (guardCfg env) frame (.memory .load) [BitVec.ofNat 256 (index*32)] = value frame v := by
  have small : (authWords env).length * 32 ≤ 65536 := by simp [authWords]
  have bound : index*32 < 2^256 := by simp [authWords] at within; omega
  have expansion : expand (authArena env) (index*32) 32 = .ok (authArena env) := by
    rw [Nat.mul_comm index]
    exact expandArenaRange (authWords env) index 1 within (by omega)
  have loaded : readWord (authArena env) (index*32) = v := by
    rw [Nat.mul_comm index]
    exact readArenaWord (authWords env) index v cell
  change (expand frame.memory (BitVec.ofNat 256 (index*32)).toNat 32 >>= _) = _
  simp only [BitVec.toNat_ofNat,Nat.mod_eq_of_lt bound,active,expansion,Bind.bind,Except.bind,loaded]
  change value {frame with memory := authArena env} v = value frame v
  rw [← active]

theorem nonceOwnerClean (env : World.Environment) (canonical : (nonceOwner env).toNat < 2^160) :
    ((nonceOwner env).setWidth 160).setWidth 256 = nonceOwner env := by
  apply BitVec.eq_of_toNat_eq
  simp [BitVec.toNat_setWidth, Nat.mod_eq_of_lt canonical, Nat.mod_eq_of_lt (nonceOwner env).isLt]

theorem mirNonceAdd (env : World.Environment) (journal : Journal) (frame : Frame)
    (nonzero : nonceNext env journal ≠ 0#256) :
    operation (guardCfg env) frame (.checked .add 256 false) [nonceValue env journal,1#256] =
      value frame (nonceNext env journal) := by
  have within := (nonceValue env journal).isLt
  have small : (nonceValue env journal).toNat + 1 < 2^256 := by
    apply Nat.lt_of_not_ge
    intro wrap
    have eq : (nonceValue env journal).toNat + 1 = 2^256 := by omega
    apply nonzero
    apply BitVec.eq_of_toNat_eq
    simp [nonceNext,BitVec.toNat_add,eq]
  have noCarry : (nonceValue env journal).uaddOverflow 1#256 = false := by
    simp [BitVec.uaddOverflow, Nat.not_le_of_gt small]
  have checked : MIR.checked .add false (nonceValue env journal) 1#256 =
      .returned (nonceNext env journal) := by simp [MIR.checked,noCarry,nonceNext]
  unfold operation
  simp only [wordBits_full,checked]
  rfl
"""


def fragments(functions, catalog):
    yield DEFINITIONS
    for side, func in zip(("before", "after"), functions, strict=True):
        branch = func["blocks"][0]["terminator"]["then"]
        block = func["blocks"][branch]
        rows = block["instructions"]
        operations = [r["operation"].get("name", r["operation"]["kind"]) for r in rows]
        require(
            operations[:9]
            == [
                "pointer_add",
                "mload",
                "mload",
                "low_bits",
                "word",
                "mapping_slot",
                "sload",
                "checked",
                "sstore",
            ]
            and not block["phis"],
            "unexpected typed authorization nonce block",
        )
        ids = [row["result"] for row in rows[:8]]
        cfg = "(guardCfg env)"
        arena_start = "authArena env"
        vals = [
            "scalar 256 224",
            "scalar 256 (suppliedNonce env)",
            "scalar 256 (nonceOwner env)",
            "scalar 160 (nonceOwner env)",
            "scalar 256 (nonceOwner env)",
            "scalar 256 (nonceKey env)",
            "scalar 256 (nonceValue env journal)",
            "scalar 256 (nonceNext env journal)",
        ]
        block_name = f"{side}AuthNonceBlock"
        yield f"""
theorem {block_name} :
    {side}AuthBody.blocks[{branch}]? = some {mir_state.block(block, catalog)} := rfl
"""
        names = [f"{side}NonceAt{i}" for i in range(9)]
        asm = """(env : World.Environment) (journal : Journal)
    (live : expiry env = 0#256) (canonical : (nonceOwner env).toNat < 2^160)
    (noOverflow : nonceNext env journal ≠ 0#256)"""
        for i, name in enumerate(names):
            prev = names[i - 1] if i else f"{side}AfterGuard"
            current = f"{prev} env journal"
            out = f"""
def {name} (env : World.Environment) (journal : Journal) : Machine :=
  let prior := {current}
  {{prior with cursor := {{prior.cursor with entered := true, next := {i}}}"""
            if i:
                mods = {
                    6: ", memory := nonceArena env",
                    7: ", «world» := nonceWarmed env journal",
                }.get(i, "")
                out += (
                    f", frame := {{prior.frame with locals := "
                    f"({ids[i - 1]}, {vals[i - 1]}) :: prior.frame.locals{mods}}}"
                )
            out += "}\n"
            out += f"""
set_option maxHeartbeats 250000 in
theorem {name}Step {asm} (ticks : Nat) :
    transition ticks {cfg} {side}AuthFunctions ({current}) =
      .ok (.next 0 ({name} env journal)) := by
  have accepted : expiry env = (0 : Word) := live
"""
            facts = []
            if i in (2, 3):
                index = 7 if i == 2 else 4
                loaded = "suppliedNonce env" if i == 2 else "nonceOwner env"
                out += f"""  have fetched (f : Frame) (mem : f.memory = {arena_start}) :
      operation {cfg} f (.memory .load) [{index * 32}#256] = value f ({loaded}) :=
    nonceArenaLoad env f {index} _ (by simp [authWords]) rfl mem
  simp only [guardCfg,mirAvailable,value] at fetched
"""
                facts.append("fetched")
            if i == 4:
                facts += ["operation", "narrow", "wordBits"]
            if i == 5:
                out += """  have clean := nonceOwnerClean env canonical
  have wide : widenBits (wordBits 160 (nonceOwner env)) = nonceOwner env := clean
"""
                facts += ["wide", "operation", "Scalar.word"]
            if i == 6:
                out += f"""  have mapped (f : Frame) (mem : f.memory = {arena_start}) :
      operation {cfg} f (.slot .mapping) [nonceOwner env,7#256] =
        value {{f with memory := nonceArena env}} (nonceKey env) :=
    mapArena {cfg} f 0 0 (nonceOwner env) 7 ((authWords env).drop 2) (by simp [authWords]) mem
  simp only [guardCfg,mirAvailable,value] at mapped
  have wide : widenBits (wordBits 160 (nonceOwner env)) = nonceOwner env :=
    nonceOwnerClean env canonical
"""
                facts += ["mapped", "wide", "Scalar.word"]
            if i == 7:
                facts += ["readMarketStorage", "nonceValue", "nonceWarmed"]
            if i == 8:
                out += """  have added := fun f => mirNonceAdd env journal f noOverflow
  simp only [guardCfg,mirAvailable,value] at added
"""
                facts.append("added")
            out += f"""  simp (config := {{maxSteps := 50000}}) [transition,{side}AuthLookup,{block_name},
    {",".join(names[: i + 1])},{side}AfterGuard,{side}AfterRead,authorizationFrame,signatureArgs,
    accepted,guardCfg,mirAvailable,required,next,advance,enter,instruction,MIR.State.read,readValue,
    readBits,assign,assignValue,put,Value.fits,Value.word,ValueType.width,ValueType.same,
    Bind.bind,Except.bind,Pure.pure,Except.pure,List.filter_cons,scalar,Scalar.ofWord,value,effect
    {",".join(("", *facts))}] <;> rfl
"""
            yield out
        count = len(names)
        pc_vars = "\n".join(
            f"  refine Execution.Trace.segment_next {count - i - 1} "
            f"(work+{count - i - 1}) (next := {name} env journal) ?_ ?_\n"
            f"  · simp only [{name}Step env journal live canonical noOverflow, "
            "Bind.bind, Except.bind, Pure.pure, Except.pure]"
            for i, name in enumerate(names)
        )
        yield f"""
theorem {side}_nonce_store_operands {asm} (work : Nat) :
    ∃ reached,
      mirStepsFrom {cfg} {side}AuthFunctions
        ⟨{{function := {func["id"]}}},[],authorizationFrame env journal⟩
        (7+{count}) (work+{count}+7) =
        .ok (.inr reached) ∧
      MIR.State.read reached.frame ⟨.local {ids[5]},.word 256⟩ = .ok (nonceKey env) ∧
      MIR.State.read reached.frame ⟨.local {ids[7]},.word 256⟩ = .ok (nonceNext env journal) ∧
      reached.frame.memory = (nonceStoreFrame env journal).memory ∧
      reached.frame.world = nonceWarmed env journal := by
  refine ⟨{names[-1]} env journal, ?_, rfl, rfl, rfl, rfl⟩
  rw [mirStepsContinue 7 {count} _ ({side}_deadline_guard env journal (work+{count})).1,
    Nat.add_sub_cancel]
  unfold mirStepsFrom
{pc_vars}
  rfl

#print axioms {side}_nonce_store_operands
"""
