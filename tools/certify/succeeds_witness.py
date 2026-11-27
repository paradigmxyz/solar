"""The succeeds witness of a specification without succeeds-if conditions.

A spec whose functions declare no @custom:succeeds-if allows every revert, so the published
statement's non-vacuity (a revert the allowed statuses reject, Generic.Spec.Covered) cannot hold of
the spec as exported, and such a run was refused. Instead the driver names one concrete call, the
witness: a spec'd function on given argument words with no value, from the constructor's slots
written with given values. The statement then strengthens the exported allowed statuses and
postcondition by one clause, the same in both:

    call = W → (∃ fs ∈ spec.specs, ∃ args pre, Pre spec fs (ctx call) pre call args) →
      obs.success = true

so a run of W from a pre-state meeting a function spec's precondition must succeed. The clause only
removes behaviours (it never weakens the spec), and its non-vacuity needs a proof that W meets that
precondition (the Shape module's pre_sat theorem for the witness). That W succeeds is proved in the
Shape module by the kernel's evaluation of SolidCore on W (kernel_rfl); the driver's choice of W
(generic.py's synth_witness, from the probe's untrusted evaluation) is never trusted.

generic.py writes shape.json's "succeeds_witness" {function, sel, arity, args, storage}; shape_gen.py
and final.py both build the clause with `strengthen`, so the proof module and the statement speak of
the same term.
"""

WORD = 2**256


def check(sw):
    """The fields of shape.json's succeeds_witness, validated; raises ValueError."""
    if not isinstance(sw, dict) or set(sw) != {"function", "sel", "arity", "args", "storage"}:
        raise ValueError("succeeds_witness must be {function, sel, arity, args, storage}")
    f, sel, n, args, st = sw["function"], sw["sel"], sw["arity"], sw["args"], sw["storage"]
    if not (isinstance(f, str) and f.isidentifier()):
        raise ValueError("succeeds_witness.function must be a Solidity name")
    if not (isinstance(sel, int) and 0 <= sel < 2**32 and isinstance(n, int) and 0 <= n < 64):
        raise ValueError("succeeds_witness: sel and arity")
    if not (
        isinstance(args, list)
        and len(args) == n
        and all(isinstance(w, int) and 0 <= w < WORD for w in args)
    ):
        raise ValueError("succeeds_witness.args must be arity words below 2^256")
    if not (isinstance(st, list) and all(isinstance(w, int) and 0 <= w < WORD for w in st)):
        raise ValueError("succeeds_witness.storage must be words below 2^256")
    return f, sel, n, args, st


def call_term(sw, storage_lit):
    """The witness call: its selector on its words, no value, the constructor's slots (STORAGE_LIT,
    a `List (Nat × Nat)` term) written with 0. Only the all-zero witness is supported so far: the
    validity and precondition proofs are the zero call's (valid_of_find, spec_pre_zero)."""
    _, sel, n, args, st = check(sw)
    if any(args) or any(st):
        raise ValueError("succeeds_witness: only the zero call is supported")
    return (
        f"⟨_root_.SolidCore.Refinement.Abi.calldata {sel} (List.replicate {n} 0), 0, "
        f"_root_.SolidCore.Refinement.Shapes.zeros ({storage_lit})⟩"
    )


def domain_term(bundle, ctx):
    """Some function spec's precondition holds of `call` (a bound variable of the clause)."""
    return (
        f"∃ fs ∈ (_root_.{bundle}).spec.specs, ∃ args pre, "
        f"_root_.SolidCore.Spec.Pre _root_.{bundle} fs (({ctx}) call) pre call args"
    )


def strengthen(base, sw, storage_lit, bundle, ctx):
    """BASE (a `Call → Observable → Prop` term) with the witness clause, as a term without colons."""
    return (
        f"fun call obs => ({base}) call obs ∧ (call = {call_term(sw, storage_lit)} → "
        f"({domain_term(bundle, ctx)}) → obs.success = true)"
    )


def describe(sw):
    f, _, n, _, _ = check(sw)
    return (
        f"the succeeds witness {f} on {n} zero word(s) with no value, from the constructor's slots "
        "written with 0: its precondition holds there, and the statement's allowed statuses and "
        "postcondition (the export's, each with the witness clause) require it to succeed"
    )
