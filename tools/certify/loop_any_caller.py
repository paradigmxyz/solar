#!/usr/bin/env python3
"""Phase 2, first slice: a loop function's translation and property theorems for EVERY caller,
bound to one run's own spec and bytes and checked by Lean before anything is reported.

    loop_any_caller.py SOURCE.sol --contract C --solar SOLAR --verify SOLAR_VERIFY --lean 'CMD'
        [--ns NAMESPACE] [--out FILE] [--work DIR]

One run (certify):

1. solar compiles the source (--emit=bin-runtime, the driver's options): the run's bytes.
2. solar-verify exports the same source's spec (--emit-lean-spec, its manifest bound to the bytes'
   sha256 with --lean-spec-bytecode-sha256); export_check.py requires the whole module, its
   manifest recording this source's sha256 (of the copy compiled) and the run's bytes' sha256, so a
   wrong digest is refused before any Lean.
3. The generator writes ONE self-contained module: the export inlined verbatim (its bundle is the
   B every property theorem reads), the run's bytes as the literal runtimeBytes, and
   runtime_is_fresh : <the proofs' pinned bytes> = runtimeBytes (decide +kernel). Every statement
   speaks of code := runtimeCode runtimeBytes. Nothing is imported in place of either: no fixed
   spec module, and no fixed byte constant in any statement. Two guards bind the export:
   <fn>_spec_selector : <fn>_spec.selector = <the source's selector> (decide; a mis-tagged export,
   whose spec would cover no call of the function, fails here) and manifest_is_the_runs (the
   inlined manifest records the run's source and bytes sha256s, rfl).
4. audit.sh scan, then CMD (a Lean checker run on the file) must exit 0 with no sorry. Otherwise
   the run is refused (Refused, exit 3) with the declaration the kernel rejected. Without CMD
   nothing is certified (exit 4).

The module's theorems, each quantified over every caller x with its assumptions in the statement
(x < 2 ^ 160, x != the contract's address):

  <fn>_translation_from  translation preservation: EthLib's run of the run's bytes sent by x is
                         SolidCore's run of the source sent by x. Its proof is the profile's
                         property-free run theorem (for Counter, Verify.Phase2.CounterRun) and
                         runtime_is_fresh: no property is in its statement or its proof;
  <fn>_property_from     property satisfaction: every source result sent by x meets the run's
                         exported spec read in x's own context (no bytecode in it);

and, for a non-payable function, the same pair for calls with a value v != 0 (the rejection), plus
<fn>_bytes_meet_from, the two composed. The property's tree half is proved by tree_meets_fixed with
the caller a free variable, so the kernel decides it for every caller at once: a spec the code does
not meet (ensures false), or one that holds only for some callers (ensures msg.sender == 0x2000),
fails there and the run is refused; nothing weaker (no fixed-caller claim) is published instead.

The refusal says "every-caller proof failed". It says the property is caller-dependent only with a
witness: when the selector's clauses reference msg.sender, a second, unpublished module proves the
same tree half for the fixed caller 0x2000 (WITNESS_CALLER); if Lean accepts that while the
every-caller proof fails, the refusal names 0x2000 as the witness. Without that, a failure is only a
failure (ensures false && msg.sender == msg.sender fails for every caller).

Sender label (sender_label, the record's route): whether msg.sender (.env .sender) occurs in the
clauses that apply to the function's selector: the requires, ensures and succeeds-if of every
function spec with that selector, the contract invariants, and the bodies of the spec functions
those clauses call, transitively. It is a syntactic fact about those clauses, read from the
export's terms; other selectors' specs are not read (specs_meet_from discharges them by selector).
It does not change the module or the theorem: either way the kernel decides every-caller
satisfaction. "no-sender-reference" means exactly: no sender reference in this selector's clauses.

Refused before any Lean: succeeds-if conditions, more than one argument or a non-uint256 argument,
two function specs with one selector, an export that is not whole or not of this source. A payable
function keeps only the zero-value pair.

Scope (stated in the module): one function's runtime code, not deployment; the spec is read in
ctxOfCallFrom x (contract 0x1000, timestamp 1); SolidCore's runtime and EthLib's run each use their
own frame (SolidCore's inherits Contract.context's timestamp 0) and no claim relates those
timestamps. The source model (the profile's checked Lean contract) is the frontend trust boundary;
the composed theorem is about the run's bytes and the run's spec.

Hook: generic.py's loop route (main's loop branch, beside loop_cuts) calls from_driver, which runs
certify for a loop function a profile covers and adds one record entry, loop_any_caller, without
changing the run's verdict; it checks with the command LOOP_ANY_CALLER_LEAN names and, without
one, records that it did not run.
"""

import argparse
import hashlib
import json
import os
import pathlib
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent

WITNESS_CALLER = 0x2000

EVERY_CALLER_FAILED = (
    "every-caller proof failed: tree_meets_fixed with the caller free does not prove the exported "
    "spec of the function's tree, so it does not certify from every caller"
)

CALLER_DEPENDENT = (
    f"{EVERY_CALLER_FAILED}; caller-dependent, witness {WITNESS_CALLER:#x}: the same proof for the "
    f"fixed caller {WITNESS_CALLER:#x} checks"
)

NO_WITNESS = (
    f"{EVERY_CALLER_FAILED}; its proof for the fixed caller {WITNESS_CALLER:#x} fails too, so there is no "
    "caller-dependence witness"
)

OPTIONS = ["-Ogas", "--evm-version", "osaka"]

ROUTES = {
    "no-sender-reference": "no sender reference in this selector's clauses",
    "sender-reference": "msg.sender occurs in this selector's clauses",
}

SENDER = re.compile(r"\.env\s+\.sender\b")
CALL = re.compile(r'\.call\s+"((?:[^"\\]|\\.)*)"')
STRING = re.compile(r'"(?:[^"\\]|\\.)*"')

COUNTER = {
    "ns": "Verify.Phase2.CounterCaller",
    "title": "Counter.bump",
    "contract": "Counter",
    "final": "Certified.counter_loop.Final",
    "fn": "bump",
    "spec_name": "Counter.bump",
    "src": "Contracts.VerifyCounter.Source",
    "selname": "bumpSel",
    # bumpSel's value (keccak256("bump(uint256)")[:4]); the module's bump_spec_selector checks it.
    "selector": 0xB20EB4C4,
    "tree": "Verify.LoopSource.Equiv.CounterObl.T",
    "body": "Verify.LoopSource.Equiv.CounterCtx.bodyRunsFrom",
    "treeeq": "Verify.LoopSource.Equiv.CounterCtx.treeFrom",
    "call_ns": "Verify.LoopSource.MirLink",
    "call": "bumpCall",
    "argsok": "Verify.LoopSource.MirLink.args_one",
    "bytes": "SolarTV.Export.Counter.bytecode",
    "gas": "Verify.LoopSource.Equiv.CounterCtx.G",
    "translation_module": "Verify.Phase2.CounterRun",
    "translation": "Verify.Phase2.CounterRun.counter_run_any",
    "value": {
        "module": "Verify.AllValue.CounterValue",
        "callv": "bumpCallV",
        "g0": "G0",
        "nonpayable": "Verify.AllValue.CounterValue.counter_nonpayable",
        "rejects": "Verify.AllValue.CounterValue.source_rejects",
    },
}

PROFILES = [COUNTER]

BQ = "\u0060"
NO_SENDER_DOC = (
    "no clause of this\n  selector (requires, ensures, succeeds-if, invariants, the spec functions they call)\n"
    "  references @@msg.sender@@"
)
SENDER_DOC = (
    "a clause of this\n  selector references @@msg.sender@@, which the statement reads as the quantified "
    "caller @@x@@"
)
NUMBER = {
    1: "one",
    2: "two",
    3: "three",
    4: "four",
    5: "five",
    6: "six",
    7: "seven",
    8: "eight",
}
EXPORT_SLOT = "\u0000EXPORT\u0000"

TEMPLATE = r"""-- Generated by solar's tools/certify/loop_any_caller.py from one run. Do not edit.
-- Spec: solar-verify's export of the run's source (sha256 @SRC_SHA@), inlined below.
-- Bytes: the runtime bytecode solar emitted for that source in the same run (sha256 @BYTES_SHA@).
import Verify.Spec.Contract
import Verify.Phase2.CallerFrom
import @TRANSLATION_MODULE@
@VALUE{import @VALUE_MODULE@
@}VALUE
@EXPORT_SLOT@
/-!
# @TITLE@ from every caller: translation and property, as separate theorems

This module is one run's: BQ_BBQ_ is the bundle solar-verify exported from the run's source (inlined
above), and BQ_codeBQ_ is the runtime bytecode solar emitted for that source in the same run
(BQ_runtimeBytesBQ_). The translation proofs are about the bytes BQ_@BYTES@BQ_;
BQ_runtime_is_freshBQ_ (the kernel's evaluation of the equality of the two literals) makes them this
run's, and no statement names BQ_@BYTES@BQ_.

BQ_@FN@_spec_selectorBQ_ checks that the export puts BQ_@FN@BQ_'s spec at the source's selector, and
BQ_manifest_is_the_runsBQ_ that the inlined manifest records the run's source and bytes SHA-256s.

The loop route's published statement for @CONTRACT@ (BQ_@FINAL@BQ_) and the spec bridge
(BQ_Verify/Spec/TreeLoop.leanBQ_) are about one caller, BQ_0x2000BQ_. Here both halves are stated for
every caller BQ_xBQ_, with the caller's assumptions in the statement: BQ_x < 2 ^ 160BQ_ (an address) and
BQ_x ≠ selfAddressBQ_ (not the contract, BQ_0x1000BQ_).

Frames: the spec is read in BQ_ctxOfCallFrom xBQ_ (contract BQ_0x1000BQ_, the call's value, timestamp 1).
SolidCore's runtime code and EthLib's run of the bytes each run in their own frame (SolidCore's
inherits the timestamp 0 of BQ_Contract.contextBQ_); the statements relate their observables only and
make no claim relating those timestamps.

Calls without value (BQ_@FN@(a)BQ_, any storage, any BQ_a < 2 ^ 256BQ_):

* BQ_@FN@_translation_fromBQ_ (**translation preservation**): EthLib's run of the run's bytes sent by
  BQ_xBQ_ is SolidCore's run of the source sent by BQ_xBQ_ at every gas from BQ_G aBQ_ below BQ_2 ^ 256BQ_ (an
  empty range when BQ_G a ≥ 2 ^ 256BQ_), and below BQ_G aBQ_ it is that result or an inert failure
  (BQ_Shapes.InertBQ_: the failure's shape, not its cause). No property is in its statement, and its
  proof is the property-free BQ_@TRANSLATION@BQ_ with BQ_runtime_is_freshBQ_.
* BQ_@FN@_property_fromBQ_ (**property satisfaction**): every result of SolidCore's runtime code sent
  by BQ_xBQ_ meets the run's exported spec (BQ_bundlePostBQ_, BQ_bundleAllowedBQ_) read in BQ_xBQ_'s own
  context BQ_ctxOfCallFrom xBQ_. No bytecode is in it. The tree half, BQ_@FN@_meets_fromBQ_, is proved with
  the caller a free variable; @SENDERDOC@.

@VALUE{Calls with a value BQ_v ≠ 0BQ_ (BQ_@FN@BQ_ is non-payable; BQ_payable := falseBQ_ in the export):

* BQ_@FN@_value_translation_fromBQ_: the source rejects and the run's bytes revert to the same
  observable from BQ_@G0@BQ_ gas on (BQ_@NONPAYABLE@BQ_).
* BQ_@FN@_value_property_fromBQ_: the source's result is that rejection, and it meets the exported spec
  read in BQ_xBQ_'s context with value BQ_vBQ_ (no function spec has succeeds-if conditions).

@}VALUE@NOVALUE@BQ_@FN@_bytes_meet_fromBQ_ composes the two zero-value theorems: the result of the run's bytes from
BQ_xBQ_ meets the run's spec. All of it is runtime code only; deployment is outside this module.
-/

namespace @NS@

open SolarTV SolidCore.Refinement SolidCore.Refinement.Generic SolidCore.Spec
open SolidCore.Verify.AnyCaller SolidCore.Verify.Generic Verify.LoopSource.Generic Verify.Phase2.CallerFrom
open SolidCore.Refinement.Shapes (bundlePost bundleAllowed idStep Inert)
open @SRC@ (contract? fuel @SELNAME@)
open @CALL_NS@ (@CALL@)
@VALUE{open @VALUE_MODULE@ (@CALLV@ @G0@)
@}VALUE
/-- solar-verify's export of the contract's spec, from this run's source (inlined above). -/
abbrev B : Bundle := @BUNDLE@

/-- BQ_@FN@BQ_'s exported spec. -/
def @FN@_spec : FnSpec := B.spec.specs[@IDX@]'(by decide)

/-- **The export puts BQ_@FN@BQ_'s spec at the source's selector BQ_@SELNAME@BQ_.** A mis-tagged export (this
spec at another selector, so that it would cover no call of BQ_@FN@BQ_) fails here. -/
theorem @FN@_spec_selector : @FN@_spec.selector = @SELNAME@ := by decide

/-- **The inlined manifest is this run's**: it records the run's source and bytes SHA-256s, the ones
in this module's header. -/
theorem manifest_is_the_runs :
    @MANIFEST@.sourceSha256 =
        "@SRC_SHA@" ∧
      @MANIFEST@.bytecodeSha256 =
        "@BYTES_SHA@" := ⟨rfl, rfl⟩

/-- The runtime bytes solar emitted for this run's source (BQ_--emit=bin-runtimeBQ_). -/
def runtimeBytes : List Nat :=
@BYTELIT@

/-- **The translation proofs' bytes are this run's bytes**: the kernel evaluates the equality of the
two literals. -/
theorem runtime_is_fresh : @BYTES@ = runtimeBytes := by decide +kernel

/-- The exact runtime bytes the statements speak of: this run's. -/
abbrev code : ByteArray := runtimeCode runtimeBytes

theorem code_eq : runtimeCode @BYTES@ = code := congrArg runtimeCode runtime_is_fresh

/-- The tree SolidCore's runtime code runs BQ_@FN@BQ_ to from every caller
(BQ_@TREEEQ@BQ_). -/
abbrev T : Tree := @TREE@

/-! ## Property satisfaction -/

/-- **BQ_@FN@BQ_'s exported spec holds of its tree for every caller**, each in its own context. -/
theorem @FN@_meets_from : ∀ a, a < 2 ^ 160 → a ≠ selfAddress →
    TreeMeetsFixedAt a B @FN@_spec (ctxAt a 0) 1 T := by
  intro a ha hne
  have hca : a % 2 ^ 256 = a := Nat.mod_eq_of_lt (by omega)
  tree_meets_fixed [T, @TREE@, @FN@_spec, B,
    @BUNDLE@, SolidCore.Spec.ctxAt, hca,
    SolidCore.Refinement.Generic.UnOp.eval]

/-- Every function spec of BQ_@FN@BQ_'s selector holds of the tree for every caller (@OTHERS@). -/
theorem specs_meet_from : ∀ a, a < 2 ^ 160 → a ≠ selfAddress → ∀ fs ∈ B.spec.specs,
    fs.selector = @SELNAME@ → TreeMeetsFixedAt a B fs (ctxAt a 0) 1 T := by
  intro a ha hne fs hfs hs
  simp only [B, @BUNDLE@, List.mem_cons, List.not_mem_nil,
    or_false] at hfs
@CASES@
/-- **Theorem 2, property satisfaction, for every caller.** For the checked contract, every caller
BQ_x < 2 ^ 160BQ_ other than the contract, every storage BQ_σBQ_ and every BQ_a < 2 ^ 256BQ_: every result of
SolidCore's runtime code sent by BQ_xBQ_ on BQ_@FN@(a)BQ_ (no value) meets the run's exported
postcondition and allowed statuses, read in BQ_xBQ_'s context BQ_ctxOfCallFrom xBQ_. -/
theorem @FN@_property_from : ∀ c, contract? = .ok c →
    ∀ x : Nat, x < 2 ^ 160 → x ≠ selfAddress →
    ∀ (σ : List (Nat × Nat)) (a : Nat), a < 2 ^ 256 →
    ∀ obs, (Loop.solidCoreRuntimeFrom x fuel).run c (@CALL@ σ a) = some obs →
      bundlePost B (ctxOfCallFrom x) idStep (@CALL@ σ a) obs ∧
        bundleAllowed B (ctxOfCallFrom x) (@CALL@ σ a) obs := by
  intro c hc x hx hne σ a ha obs hrun
  exact property_from (by decide) (@SRC@.shape_of hc).2.2 (ws := [256]) rfl
    rfl (@BODY@ hc) @TREEEQ@
    specs_meet_from x hx hne _ ⟨σ, [a], @ARGSOK@ ha, rfl⟩ obs hrun

/-! ## Translation preservation -/

/-- **Theorem 1, translation preservation, for every caller.** For the checked contract, every
caller BQ_x < 2 ^ 160BQ_ other than the contract, every storage BQ_σBQ_ and every BQ_a < 2 ^ 256BQ_:
SolidCore's runtime code sent by BQ_xBQ_ runs BQ_@FN@(a)BQ_ to some BQ_obsBQ_; every result of EthLib's run of
the run's bytes sent by BQ_xBQ_ is BQ_obsBQ_ or, below BQ_G aBQ_ gas, an inert failure; and from BQ_G aBQ_ gas
(below BQ_2 ^ 256BQ_) it is BQ_obsBQ_. -/
theorem @FN@_translation_from : ∀ c, contract? = .ok c →
    ∀ x : Nat, x < 2 ^ 160 → x ≠ selfAddress →
    ∀ (σ : List (Nat × Nat)) (a : Nat), a < 2 ^ 256 →
    ∃ obs, (Loop.solidCoreRuntimeFrom x fuel).run c (@CALL@ σ a) = some obs ∧
      (∀ gas obs', runAt ⟨x⟩ gas code (@CALL@ σ a) = some obs' →
        obs' = obs ∨ (gas < @GAS@ a ∧ Inert (@CALL@ σ a) obs')) ∧
      (∀ gas, @GAS@ a ≤ gas → gas < 2 ^ 256 →
        runAt ⟨x⟩ gas code (@CALL@ σ a) = some obs) := by
  intro c hc x hx hne σ a ha
  rw [← code_eq]
  obtain ⟨c', hc', h⟩ := @TRANSLATION@
  obtain rfl : c' = c := Except.ok.inj (hc'.symm.trans hc)
  obtain ⟨obs, h1, h2, h3⟩ := h ⟨x⟩ ⟨hx, hne⟩ σ a ha
  exact ⟨obs, h1, h2, h3⟩

@VALUE{/-! ## Calls with a value -/

/-- **Theorem 1 with a value: translation preservation of the rejection, for every caller.** -/
theorem @FN@_value_translation_from : ∀ c, contract? = .ok c →
    ∀ x : Nat, x < 2 ^ 160 → x ≠ selfAddress →
    ∀ (σ : List (Nat × Nat)) (a v : Nat), a < 2 ^ 256 → v < 2 ^ 256 → v ≠ 0 →
      (Loop.solidCoreRuntimeFrom x fuel).run c (@CALLV@ σ a v) = some (Abi.rejected σ) ∧
        ∀ gas, @G0@ ≤ gas → runAt ⟨x⟩ gas code (@CALLV@ σ a v) = some (Abi.rejected σ) := by
  intro c hc x hx hne σ a v ha hv hv0
  rw [← code_eq]
  exact @NONPAYABLE@ c hc ⟨x⟩ ⟨hx, hne⟩ σ a v ha hv hv0

/-- No function spec of the export has succeeds-if conditions. -/
theorem no_succeeds : ∀ fs ∈ B.spec.specs, fs.succeedsIf = [] := by
  simp [B, @BUNDLE@]

/-- **Theorem 2 with a value: property satisfaction, for every caller.** Every result of the source
sent by BQ_xBQ_ on BQ_@FN@(a)BQ_ with a value BQ_v ≠ 0BQ_ is the rejection (failure, no output, no logs, storage
BQ_σBQ_), and it meets the exported spec read in BQ_xBQ_'s context with value BQ_vBQ_. -/
theorem @FN@_value_property_from : ∀ c, contract? = .ok c →
    ∀ x : Nat, x < 2 ^ 160 → x ≠ selfAddress →
    ∀ (σ : List (Nat × Nat)) (a v : Nat), a < 2 ^ 256 → v < 2 ^ 256 → v ≠ 0 →
    ∀ obs, (Loop.solidCoreRuntimeFrom x fuel).run c (@CALLV@ σ a v) = some obs →
      obs = Abi.rejected σ ∧ bundlePost B (ctxOfCallFrom x) idStep (@CALLV@ σ a v) obs ∧
        bundleAllowed B (ctxOfCallFrom x) (@CALLV@ σ a v) obs := by
  intro c hc x hx hne σ a v ha hv hv0 obs hrun
  have h := @REJECTS@ hc x σ ha hv hv0
  have hrun' : Loop.runtimeRunFrom x fuel c (@CALLV@ σ a v) = some obs := hrun
  rw [h] at hrun'
  obtain rfl := Option.some.inj hrun'
  exact ⟨rfl, fail_meets no_succeeds _ _ _ rfl⟩

@}VALUE/-! ## The two halves composed -/

/-- **The run's bytes meet the run's spec from every caller**: BQ_@FN@_translation_fromBQ_ and
BQ_@FN@_property_fromBQ_ composed. From BQ_G aBQ_ gas (below BQ_2 ^ 256BQ_) EthLib's run of the bytes sent by
BQ_xBQ_ has a result meeting the exported spec in BQ_xBQ_'s context; at any gas, every result meets it or
is inert. -/
theorem @FN@_bytes_meet_from : ∀ c, contract? = .ok c →
    ∀ x : Nat, x < 2 ^ 160 → x ≠ selfAddress →
    ∀ (σ : List (Nat × Nat)) (a : Nat), a < 2 ^ 256 →
      (∀ gas, @GAS@ a ≤ gas → gas < 2 ^ 256 →
        ∃ obs, runAt ⟨x⟩ gas code (@CALL@ σ a) = some obs ∧
          bundlePost B (ctxOfCallFrom x) idStep (@CALL@ σ a) obs) ∧
      (∀ gas obs, runAt ⟨x⟩ gas code (@CALL@ σ a) = some obs →
        bundlePost B (ctxOfCallFrom x) idStep (@CALL@ σ a) obs ∨ Inert (@CALL@ σ a) obs) := by
  intro c hc x hx hne σ a ha
  obtain ⟨obs, h1, h2, h3⟩ := @FN@_translation_from c hc x hx hne σ a ha
  have hp := (@FN@_property_from c hc x hx hne σ a ha obs h1).1
  refine ⟨fun gas hg hlt => ⟨obs, h3 gas hg hlt, hp⟩, fun gas obs' h => ?_⟩
  rcases h2 gas obs' h with rfl | ⟨-, hi⟩
  · exact Or.inl hp
  · exact Or.inr hi

end @NS@

/-- info: '@NS@.@FN@_translation_from' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms @NS@.@FN@_translation_from

/-- info: '@NS@.@FN@_property_from' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms @NS@.@FN@_property_from

@VALUE{/-- info: '@NS@.@FN@_value_translation_from' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms @NS@.@FN@_value_translation_from

/-- info: '@NS@.@FN@_value_property_from' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms @NS@.@FN@_value_property_from

@}VALUE/-- info: '@NS@.@FN@_bytes_meet_from' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms @NS@.@FN@_bytes_meet_from
""".replace("BQ_", BQ)


class Refused(Exception):
    pass


# --- the exported bundle, read as terms --------------------------------------------------------


def _close(text, i):
    """The index of the bracket closing text[i], skipping string literals."""
    depth, j, n = 0, i, len(text)
    while j < n:
        ch = text[j]
        if ch == '"':
            m = STRING.match(text, j)
            if not m:
                raise Refused("unreadable export: an unterminated string")
            j = m.end()
            continue
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
            if depth == 0:
                return j
        j += 1
    raise Refused("unreadable export: unbalanced brackets")


def _split(body):
    """BODY split at its top-level commas, each part stripped (empty parts dropped)."""
    parts, depth, start, j, n = [], 0, 0, 0, len(body)
    while j < n:
        ch = body[j]
        if ch == '"':
            m = STRING.match(body, j)
            if not m:
                raise Refused("unreadable export: an unterminated string")
            j = m.end()
            continue
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        elif ch == "," and depth == 0:
            parts.append(body[start:j])
            start = j + 1
        j += 1
    parts.append(body[start:])
    return [p.strip() for p in parts if p.strip()]


def _record(text):
    """The fields of a Lean structure instance { k := v, ... }, as raw text."""
    text = text.strip()
    if not (text.startswith("{") and _close(text, 0) == len(text) - 1):
        raise Refused("unreadable export: a structure instance expected")
    out = {}
    for part in _split(text[1:-1]):
        k, sep, v = part.partition(":=")
        if not sep:
            raise Refused(f"unreadable export: field without :=: {part[:60]!r}")
        out[k.strip()] = v.strip()
    return out


def _list(text):
    text = text.strip()
    if not (text.startswith("[") and _close(text, 0) == len(text) - 1):
        raise Refused("unreadable export: a list expected")
    return _split(text[1:-1])


def _str(text):
    m = STRING.fullmatch(text.strip())
    if not m:
        raise Refused(f"unreadable export: a string expected, not {text[:40]!r}")
    return json.loads(m.group(0))


def _field(rec, k):
    if k not in rec:
        raise Refused(f"unreadable export: no field {k}")
    return rec[k]


def parse_bundle(text, contract=None):
    """The export's one Bundle definition (CONTRACT's, when named): its name, spec functions
    (name to body), invariants and function specs (name, selector, payable, params, requires,
    ensures, succeedsIf), each clause as its raw term text."""
    defs = re.findall(r"(?m)^def (\w+) : Bundle where\n", text)
    if contract is not None:
        defs = [d for d in defs if d == contract]
    if len(defs) != 1:
        raise Refused(
            f"the export does not define exactly one Bundle{' ' + contract if contract else ''}"
        )
    name = defs[0]
    i = text.index(f"def {name} : Bundle where\n")
    j = text.index("spec :=", i)
    k = text.index("{", j)
    spec = _record(text[k : _close(text, k) + 1])
    fns = {}
    for item in _list(_field(spec, "fns")):
        r = _record(item)
        fns[_str(_field(r, "name"))] = _field(r, "body")
    specs = []
    for item in _list(_field(spec, "specs")):
        r = _record(item)
        sel = _field(r, "selector")
        if not re.fullmatch(r"0x[0-9a-fA-F]+|\d+", sel):
            raise Refused("unreadable export: a selector")
        specs.append(
            {
                "name": _str(_field(r, "name")),
                "selector": int(sel, 0),
                "payable": _field(r, "payable") == "true",
                "params": _list(_field(r, "params")),
                "requires": _list(_field(r, "requires")),
                "ensures": _list(_field(r, "ensures")),
                "succeedsIf": _list(_field(r, "succeedsIf")),
            }
        )
    return {
        "name": name,
        "fns": fns,
        "invariants": _list(_field(spec, "invariants")),
        "specs": specs,
    }


def _code(term):
    return STRING.sub('""', term)


def selector_clauses(bundle, selector):
    """The clauses that apply to a call at SELECTOR: the requires, ensures and succeeds-if of every
    function spec with that selector, the invariants, and the bodies of the spec functions these
    call, transitively (a call to a function the export does not define is refused)."""
    clauses = [
        t
        for s in bundle["specs"]
        if s["selector"] == selector
        for t in s["requires"] + s["ensures"] + s["succeedsIf"]
    ] + list(bundle["invariants"])
    seen, todo = set(), [n for t in clauses for n in CALL.findall(t)]
    while todo:
        n = todo.pop()
        if n in seen:
            continue
        if n not in bundle["fns"]:
            raise Refused(
                f"a clause calls the spec function {n!r}, which the export does not define"
            )
        seen.add(n)
        clauses.append(bundle["fns"][n])
        todo += CALL.findall(bundle["fns"][n])
    return clauses


def sender_label(bundle, selector):
    """ "sender-reference" when .env .sender occurs in SELECTOR's clauses (selector_clauses), else
    "no-sender-reference". Syntactic, over those clauses only (ROUTES says what each means)."""
    hit = any(SENDER.search(_code(t)) for t in selector_clauses(bundle, selector))
    return "sender-reference" if hit else "no-sender-reference"


# --- the module --------------------------------------------------------------------------------


def others_doc(specs, idx):
    rest = [s["name"].split(".")[-1] for j, s in enumerate(specs) if j != idx]
    if not rest:
        return "no other function spec"
    quoted = [f"{BQ}{n}{BQ}" for n in rest]
    names = (
        quoted[0] if len(quoted) == 1 else ", ".join(quoted[:-1]) + " and " + quoted[-1]
    )
    if len(rest) == 1:
        return f"the other one,\n{names}, has another selector"
    return (
        f"the other {NUMBER.get(len(rest), len(rest))},\n{names}, have other selectors"
    )


def cases(specs, idx, fn):
    alts = " | ".join(["rfl"] * len(specs))
    out = [f"  rcases hfs with {alts}"]
    for j in range(len(specs)):
        out.append(
            f"  · exact {fn}_meets_from a ha hne"
            if j == idx
            else "  · exact absurd hs (by decide)"
        )
    return "\n".join(out) + "\n"


def runtime_bytes(runtime_hex):
    h = runtime_hex.strip().lower().removeprefix("0x")
    if not h or len(h) % 2 or not re.fullmatch(r"[0-9a-f]+", h):
        raise Refused("the run's runtime bytecode is not hex")
    return bytes.fromhex(h)


def byte_literal(bs):
    rows = [
        ", ".join(f"0x{b:02x}" for b in bs[i : i + 16]) for i in range(0, len(bs), 16)
    ]
    return "  [" + ",\n   ".join(rows) + "]"


def inline_export(spec_text):
    """The export without its one import line (Verify.Spec.Contract, which the module imports)."""
    lines = spec_text.splitlines(keepends=True)
    if not lines or lines[0].strip() != "import Verify.Spec.Contract":
        raise Refused("the export does not start with import Verify.Spec.Contract")
    rest = "".join(lines[1:])
    if re.search(r"(?m)^\s*import\b", rest):
        raise Refused("the export imports more than Verify.Spec.Contract")
    return rest.strip("\n") + "\n"


def spec_namespace(spec_text):
    m = re.findall(r"(?m)^namespace (\S+)$", spec_text)
    if len(m) != 1:
        raise Refused("the export does not open exactly one namespace")
    return m[0]


def manifest_digests(spec_text):
    """The export's manifest (its one def manifest : Manifest := block): its sourceSha256 and
    bytecodeSha256, each recorded exactly once there."""
    blocks = re.findall(
        r"(?ms)^def manifest : Manifest :=\n(.*?)(?:\n\s*\n|\Z)", spec_text
    )
    if len(blocks) != 1:
        raise Refused("the export does not define exactly one manifest")
    out = []
    for k in ("sourceSha256", "bytecodeSha256"):
        vs = re.findall(rf'\b{k} := "([0-9a-f]{{64}})"', blocks[0])
        if len(vs) != 1:
            raise Refused(f"the export's manifest does not record one {k}")
        out.append(vs[0])
    return tuple(out)


def any_caller_pair(spec_text, runtime_hex, profile=COUNTER, ns=None, *, source_sha256):
    """(route, module text, notes) for the profile's function, from the run's export SPEC_TEXT and
    the run's runtime bytecode RUNTIME_HEX, both inlined, for the source with SOURCE_SHA256; raises
    Refused with the reason. The text is unchecked until Lean accepts it (certify)."""
    ns = ns or profile["ns"]
    bs = runtime_bytes(runtime_hex)
    bytes_sha = hashlib.sha256(bs).hexdigest()
    msrc, mbytes = manifest_digests(spec_text)
    if msrc != source_sha256:
        raise Refused(
            f"the export's manifest is of a source with SHA-256 {msrc}, not the run's ({source_sha256})"
        )
    if mbytes != bytes_sha:
        raise Refused(
            f"the export's manifest is about bytecode with SHA-256 {mbytes}, not the run's ({bytes_sha})"
        )
    bundle = parse_bundle(spec_text, profile["contract"])
    specs = bundle["specs"]
    idx = [j for j, s in enumerate(specs) if s["name"] == profile["spec_name"]]
    if len(idx) != 1:
        raise Refused(f"{profile['spec_name']}: no single function spec of that name")
    idx = idx[0]
    fs = specs[idx]
    if any(s["selector"] == fs["selector"] for j, s in enumerate(specs) if j != idx):
        raise Refused(f"{fs['name']}: another function spec has its selector")
    want = profile.get("selector")
    if want is not None and fs["selector"] != want:
        raise Refused(
            f"{fs['name']}: the export puts it at selector {fs['selector']:#010x}, not the source's "
            f"{profile['selname']} ({want:#010x})"
        )
    if fs["params"] != ["(.uint 256)"]:
        raise Refused(
            f"{fs['name']}: the every-caller template states one uint256 argument, not {fs['params']}"
        )
    if fs["succeedsIf"]:
        raise Refused(
            f"{fs['name']}: succeeds-if conditions (the every-caller loop proof covers specs without them)"
        )
    route = sender_label(bundle, fs["selector"])
    notes = []
    value = profile.get("value")
    if fs["payable"]:
        notes.append(
            "payable: calls with a value run the body, which this slice does not cover; "
            "only calls without value are certified"
        )
        value = None
    elif value is None:
        notes.append(
            "non-payable, but the profile names no rejection theorem for calls with a value"
        )
    elif any(s["succeedsIf"] for s in specs):
        notes.append(
            "a function spec has succeeds-if conditions: calls with a value are not covered"
        )
        value = None
    if route == "sender-reference":
        notes.append(
            "msg.sender occurs in this selector's clauses: if the every-caller proof fails, the "
            f"fixed caller {WITNESS_CALLER:#x} is checked for a caller-dependence witness"
        )
    export = inline_export(spec_text)
    spec_ns = spec_namespace(spec_text)
    rep = {
        "@MANIFEST@": f"{spec_ns}.manifest",
        "@NS@": ns,
        "@TITLE@": profile["title"],
        "@CONTRACT@": profile["contract"],
        "@FINAL@": profile["final"],
        "@BUNDLE@": f"{spec_ns}.{bundle['name']}",
        "@SRC@": profile["src"],
        "@SELNAME@": profile["selname"],
        "@TREEEQ@": profile["treeeq"],
        "@TREE@": profile["tree"],
        "@BODY@": profile["body"],
        "@CALL_NS@": profile["call_ns"],
        "@ARGSOK@": profile["argsok"],
        "@BYTES@": profile["bytes"],
        "@BYTES_SHA@": bytes_sha,
        "@SRC_SHA@": source_sha256,
        "@BYTELIT@": byte_literal(bs),
        "@GAS@": profile["gas"],
        "@TRANSLATION_MODULE@": profile["translation_module"],
        "@TRANSLATION@": profile["translation"],
        "@IDX@": str(idx),
        "@OTHERS@": others_doc(specs, idx),
        "@SENDERDOC@": (
            SENDER_DOC if route == "sender-reference" else NO_SENDER_DOC
        ).replace("@@", BQ),
        "@EXPORT_SLOT@": EXPORT_SLOT,
    }
    if value:
        rep.update(
            {
                "@VALUE_MODULE@": value["module"],
                "@CALLV@": value["callv"],
                "@G0@": value["g0"],
                "@NONPAYABLE@": value["nonpayable"],
                "@REJECTS@": value["rejects"],
                "@NOVALUE@": "",
            }
        )
        text = re.sub(r"@VALUE\{(.*?)@\}VALUE", r"\1", TEMPLATE, flags=re.DOTALL)
    else:
        why = notes[0] if notes else "not covered"
        text = re.sub(r"@VALUE\{.*?@\}VALUE", "", TEMPLATE, flags=re.DOTALL)
        rep["@NOVALUE@"] = f"Calls with a value are outside this module ({why}).\n\n"
    text = text.replace("@CASES@", cases(specs, idx, profile["fn"]))
    # Longest first: @TREEEQ@ before @TREE@, @TRANSLATION_MODULE@ before @TRANSLATION@.
    for k in sorted(rep, key=len, reverse=True):
        text = text.replace(k, rep[k])
    # @CALLV@ is filled above, so @CALL@ no longer prefixes anything.
    text = text.replace("@CALL@", profile["call"]).replace("@FN@", profile["fn"])
    left = re.findall(r"@[A-Z_0-9]+@|@VALUE\{|@\}VALUE", text)
    if left:
        raise Refused(f"template placeholders left unfilled: {sorted(set(left))}")
    # The export last: its strings (such as trusted @custom:assume items) are never templated.
    return route, text.replace(EXPORT_SLOT, export), notes


# --- one run: compile, export, generate, check -------------------------------------------------


def _run(cmd, **kw):
    p = subprocess.run(cmd, check=False, capture_output=True, text=True, **kw)
    return p.returncode, p.stdout, p.stderr


def run_bytes(solar, copy, contract, options=OPTIONS):
    """solar's runtime bytecode of CONTRACT in COPY (run beside it) and solar's version."""
    rc, out, err = _run(
        [str(solar), copy.name, *options, "--emit=bin-runtime"], cwd=copy.parent
    )
    if rc != 0:
        raise Refused(f"solar cannot compile the source: {err.strip()[-300:]}")
    j = json.loads(out)
    for k, v in j.get("contracts", {}).items():
        if k.rsplit(":", 1)[1] == contract and v.get("bin-runtime"):
            return v["bin-runtime"], j.get("version", "")
    raise Refused(f"solar emitted no runtime bytecode for {contract}")


def export_spec(verify, copy, contract, module, bytes_sha, compiler):
    """solar-verify's --emit-lean-spec of COPY (run beside it, so the manifest names the file
    alone), bound to the run's bytes' sha256 BYTES_SHA, and checked by export_check.py: whole, its
    manifest recording COPY's sha256 and BYTES_SHA."""
    out = copy.parent / "Spec.lean"
    out.unlink(missing_ok=True)
    rc, _, err = _run(
        [
            str(verify),
            "--emit-lean-spec",
            out.name,
            "--lean-spec-module",
            module,
            "--lean-spec-bytecode-sha256",
            bytes_sha,
            "--lean-spec-compiler",
            compiler,
            copy.name,
        ],
        cwd=copy.parent,
    )
    if rc != 0 or not out.is_file():
        raise Refused(f"solar-verify cannot export the spec: {err.strip()[-300:]}")
    rc, _, err = _run(
        [
            sys.executable,
            "-I",
            str(HERE / "export_check.py"),
            "spec",
            str(out),
            module,
            "--source",
            str(copy),
            "--contract",
            contract,
            "--bytecode-sha256",
            bytes_sha,
        ]
    )
    if rc != 0:
        raise Refused(
            f"the export is not whole or not of this run's source and bytes: {err.strip()[-300:]}"
        )
    return out.read_text()


def declaration_at(text, line):
    """The declaration a 1-based line of TEXT belongs to."""
    name = "the module header"
    for i, l in enumerate(text.splitlines(), 1):
        if i > line:
            break
        if m := re.match(r"(?:theorem|def|abbrev|example)\s+(\S+)", l):
            name = m.group(1)
        elif l.startswith("#guard_msgs"):
            name = "the axiom guards"
    return name


def kernel_errors(text, log, path):
    """(declaration, message) for each error Lean reports in PATH (TEXT), from its LOG."""
    errs = [
        (declaration_at(text, int(m.group(1))), m.group(2).strip())
        for m in re.finditer(
            re.escape(path.name) + r":(\d+):\d+: error: (.*)$", log, re.MULTILINE
        )
    ]
    if not errs:
        errs = [("the module", (log.strip().splitlines() or ["no output"])[-1][:200])]
    return errs


def guard_failed(where):
    """Whether a failed declaration is one that binds the module to the run (bytes, manifest,
    selector), whose failure no witness explains."""
    return any(
        w in ("runtime_is_fresh", "manifest_is_the_runs")
        or w.endswith("_spec_selector")
        for w in where
    )


def kernel_refusal(text, log, path, witness=None):
    """The reason a Lean run on PATH (TEXT) failed, from its LOG. WITNESS is whether the fixed
    caller's probe checked (None: not run); only True makes the reason caller-dependent."""
    errs = kernel_errors(text, log, path)
    where = sorted({w for w, _ in errs})
    first = f"{errs[0][0]}: {errs[0][1]}"
    if "runtime_is_fresh" in where:
        return f"the run's bytes are not the bytes the translation proofs are about ({first})"
    if "manifest_is_the_runs" in where:
        return f"the inlined manifest is not of the run's source and bytes ({first})"
    if any(w.endswith("_spec_selector") for w in where):
        return f"the export puts the function's spec at another selector than the source's ({first})"
    if any(w.endswith("_meets_from") for w in where):
        if witness is True:
            return f"{CALLER_DEPENDENT} ({first})"
        if witness is False:
            return f"{NO_WITNESS} ({first})"
        return f"{EVERY_CALLER_FAILED} ({first})"
    return f"the kernel refuses the generated module at {', '.join(where)} ({first})"


WITNESS = r"""/-! ## Witness probe (checked, never published): the fixed caller @CALLER@ -/

/-- BQ_@FN@BQ_'s exported spec holds of its tree for the one caller BQ_@CALLER@BQ_. -/
theorem @FN@_meets_at_witness : ∀ a, a = @CALLER@ →
    TreeMeetsFixedAt a B @FN@_spec (ctxAt a 0) 1 T := by
  rintro a rfl
  tree_meets_fixed [T, @TREE@, @FN@_spec, B,
    @BUNDLE@, SolidCore.Spec.ctxAt,
    SolidCore.Refinement.Generic.UnOp.eval]

end @NS@

/-- info: '@NS@.@FN@_meets_at_witness' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms @NS@.@FN@_meets_at_witness
""".replace("BQ_", BQ)


def witness_module(text, profile, ns, caller=WITNESS_CALLER):
    """The probe for a caller-dependence witness: TEXT (the generated module) up to its property
    section, then the tree half for the one caller CALLER. It is checked, never published."""
    i = text.index("/-! ## Property satisfaction -/")
    m = re.search(r"(?m)^abbrev B : Bundle := (\S+)$", text)
    if not m:
        raise Refused("the generated module has no bundle B")
    rep = {
        "@CALLER@": f"{caller:#x}",
        "@TREE@": profile["tree"],
        "@BUNDLE@": m.group(1),
        "@FN@": profile["fn"],
        "@NS@": ns,
    }
    w = WITNESS
    for k, v in rep.items():
        w = w.replace(k, v)
    return text[:i] + w


def certify(
    src, contract, *, solar, verify, lean, work, profile=None, ns=None, options=OPTIONS
):
    """One run: compile, export, generate and check. Returns the record of a module Lean accepted;
    raises Refused otherwise, with the record so far as its .record."""
    src = pathlib.Path(src).resolve()
    work = pathlib.Path(work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    rec = {"source": str(src), "contract": contract}
    try:
        profile = profile or next(
            (p for p in PROFILES if p["contract"] == contract), None
        )
        if profile is None:
            raise Refused(f"no every-caller profile for {contract}")
        ns = ns or profile["ns"]
        rec["profile"] = profile["title"]
        copy = work / src.name
        shutil.copyfile(src, copy)
        # The snapshot compiled and exported, not the original (which may change meanwhile).
        rec["source_sha256"] = hashlib.sha256(copy.read_bytes()).hexdigest()
        runtime, version = run_bytes(solar, copy, contract, options)
        rec["runtime_sha256"] = hashlib.sha256(runtime_bytes(runtime)).hexdigest()
        spec = export_spec(
            verify, copy, contract, f"{ns}.Spec", rec["runtime_sha256"], version
        )
        rec["spec_sha256"] = hashlib.sha256(spec.encode()).hexdigest()
        route, text, notes = any_caller_pair(
            spec, runtime, profile, ns, source_sha256=rec["source_sha256"]
        )
        rec.update(route=route, route_meaning=ROUTES[route], notes=notes)
        mod = work / (ns.rsplit(".", 1)[-1] + ".lean")
        mod.write_text(text)
        rec["module"] = str(mod)
        rec["module_sha256"] = hashlib.sha256(text.encode()).hexdigest()
        rc, _, err = _run(["bash", str(HERE / "audit.sh"), "scan", str(mod)])
        if rc != 0:
            raise Refused(f"the generated module fails the scan: {err.strip()[-300:]}")
        if not lean:
            raise Refused(
                "no Lean checker (LOOP_ANY_CALLER_LEAN or --lean): nothing is certified unchecked"
            )
        rc, out, err = _run([*shlex.split(lean), str(mod)])
        log = out + err
        (work / "lean.log").write_text(log)
        rec["lean_rc"] = rc
        if rc != 0:
            where = {w for w, _ in kernel_errors(text, log, mod)}
            witness = None
            if (
                route == "sender-reference"
                and any(w.endswith("_meets_from") for w in where)
                and not guard_failed(where)
            ):
                probe = work / (mod.stem + "Witness.lean")
                probe.write_text(witness_module(text, profile, ns))
                prc, pout, perr = _run([*shlex.split(lean), str(probe)])
                plog = pout + perr
                (work / "witness.log").write_text(plog)
                witness = prc == 0 and not re.search(
                    r"declaration uses 'sorry'|sorryAx", plog
                )
                rec["witness"] = {
                    "caller": f"{WITNESS_CALLER:#x}",
                    "module": str(probe),
                    "lean_rc": prc,
                    "holds": witness,
                }
            raise Refused(kernel_refusal(text, log, mod, witness))
        if re.search(r"declaration uses 'sorry'|sorryAx", log):
            raise Refused("the Lean check reports sorry")
        rec["status"] = "checked"
        return rec
    except Refused as e:
        rec["status"] = "refused"
        rec["why"] = str(e)
        e.record = rec
        raise


def driver_hook(
    record, src, contract, loop_functions, *, solar, verify, work, log=print
):
    """generic.py's loop route: the every-caller module of a loop function a profile covers, from
    this run's source, spec and bytes, checked with the command LOOP_ANY_CALLER_LEAN names. SOLAR
    and VERIFY return the binaries. It sets record["loop_any_caller"] and never changes the run's
    verdict; without LOOP_ANY_CALLER_LEAN the entry says it did not run."""
    profile = next(
        (
            p
            for p in PROFILES
            if p["contract"] == contract and p["fn"] in loop_functions
        ),
        None,
    )
    if profile is None:
        return None
    lean = os.environ.get("LOOP_ANY_CALLER_LEAN")
    if not lean:
        rec = {
            "status": "not run",
            "why": "LOOP_ANY_CALLER_LEAN is not set: nothing is certified unchecked",
            "profile": profile["title"],
        }
    else:
        try:
            rec = certify(
                src,
                contract,
                solar=solar(),
                verify=verify(),
                lean=lean,
                work=pathlib.Path(work) / "any-caller",
                profile=profile,
            )
        except Refused as e:
            rec = getattr(e, "record", {"status": "refused", "why": str(e)})
        except Exception as e:  # a build of solar or solar-verify failed
            rec = {
                "status": "refused",
                "why": f"{type(e).__name__}: {e}",
                "profile": profile["title"],
            }
    record["loop_any_caller"] = rec
    log(
        f"loop any-caller ({profile['title']}): {rec['status']}"
        + (f": {rec['why']}" if "why" in rec else "")
    )
    return rec


LOOP_NODES = ("ForStatement", "WhileStatement", "DoWhileStatement")


def loop_functions(ast, contract):
    """The functions of CONTRACT (solc's AST) whose bodies contain a loop."""
    out = set()
    fns = [
        f
        for n in ast.get("nodes", [])
        if n.get("nodeType") == "ContractDefinition" and n.get("name") == contract
        for f in n.get("nodes", [])
        if f.get("nodeType") == "FunctionDefinition"
    ]
    for f in fns:
        todo = [f.get("body")]
        while todo:
            x = todo.pop()
            if isinstance(x, dict):
                if x.get("nodeType") in LOOP_NODES:
                    out.add(f.get("name"))
                    break
                todo += list(x.values())
            elif isinstance(x, list):
                todo += x
    return sorted(out)


def from_driver(record, src, ast, contract, work, build_solar, build_solar_verify, log):
    """generic.py's call site (its loop route, beside loop_cuts): CONTRACT is --contract, else the
    source's one concrete contract. See driver_hook."""
    if not contract:
        names = [
            n.get("name")
            for n in ast.get("nodes", [])
            if n.get("nodeType") == "ContractDefinition"
            and n.get("contractKind") == "contract"
            and not n.get("abstract")
        ]
        if len(names) != 1:
            return None
        contract = names[0]
    return driver_hook(
        record,
        src,
        contract,
        loop_functions(ast, contract),
        solar=build_solar,
        verify=build_solar_verify,
        work=work,
        log=log,
    )


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("source", type=pathlib.Path)
    ap.add_argument("--contract", required=True)
    ap.add_argument("--solar", required=True)
    ap.add_argument("--verify", required=True)
    ap.add_argument("--lean", default=os.environ.get("LOOP_ANY_CALLER_LEAN"))
    ap.add_argument("--ns")
    ap.add_argument("--out", type=pathlib.Path)
    ap.add_argument("--work", type=pathlib.Path)
    a = ap.parse_args(argv)
    with tempfile.TemporaryDirectory() as tmp:
        try:
            rec = certify(
                a.source,
                a.contract,
                solar=a.solar,
                verify=a.verify,
                lean=a.lean,
                work=a.work or pathlib.Path(tmp),
                ns=a.ns,
            )
        except Refused as e:
            print(f"loop_any_caller: refused: {e}", file=sys.stderr)
            return 4 if "no Lean checker" in str(e) else 3
        for n in rec["notes"]:
            print(f"loop_any_caller: note: {n}", file=sys.stderr)
        print(
            f"loop_any_caller: route {rec['route']} ({rec['route_meaning']}); checked",
            file=sys.stderr,
        )
        if a.out:
            shutil.copyfile(rec["module"], a.out)
        print(json.dumps(rec, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
