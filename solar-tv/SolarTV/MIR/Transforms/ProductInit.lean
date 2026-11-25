import SolarTV.MIR.State.Aggregates

/-! Initialization of flat products constructed with `insert_value`.

The importer lists the inserts derived from an undefined product in
dependency order, and separately lists all reads outside those inserts.
The kernel checks the initialized fields of each result and forbids any
incomplete result from reaching one of those reads. -/

namespace SolarTV.MIR.ProductInit

structure Fields where
  arity : Nat
  filled : List Nat

structure Insert where
  result : Nat
  base : Option Nat
  arity : Nat
  index : Nat

abbrev Facts := List (Nat × Fields)

def record (facts : Facts) (insert : Insert) : Option Facts := do
  if insert.index ≥ insert.arity || (facts.lookup insert.result).isSome then none else
  let base ← match insert.base with
    | none => some ⟨insert.arity, []⟩
    | some id => facts.lookup id
  if base.arity != insert.arity then none else
  return (insert.result, ⟨base.arity, insert.index :: base.filled⟩) :: facts

def complete (fields : Fields) : Bool :=
  (List.range fields.arity).all fields.filled.contains

def safe (inserts : List Insert) (reads : List Nat) : Bool :=
  match inserts.foldlM record [] with
  | none => false
  | some facts => reads.all (fun id => (facts.lookup id).elim true complete)

def Initialized (fields : List State.Value) (filled : List Nat) : Prop :=
  ∀ i ∈ filled, ∃ value, fields[i]? = some (State.Value.scalar value)

/-- Inserting a scalar defines its selected field. Every previously defined
field stays defined, including when the same field is written again. -/
theorem set_initialized (fields : List State.Value) (filled : List Nat)
    (index : Nat) (value : State.Scalar) (fits : index < fields.length)
    (before : Initialized fields filled) :
    Initialized (fields.set index (.scalar value)) (index :: filled) := by
  intro i member
  by_cases same : i = index
  · subst i
    exact ⟨value, List.getElem?_set_self fits⟩
  · have old := before i (by simpa [same] using member)
    simpa [List.getElem?_set_ne (Ne.symm same)] using old

/-- The field mask tracks the actual MIR insert semantics for an undefined
seed or a previously constructed product. -/
theorem insert_initialized (types : List State.ValueType) (base : State.Value)
    (fields : List State.Value) (filled : List Nat) (index : Nat)
    (value : State.Scalar) (arity : fields.length = types.length)
    (fits : index < types.length)
    (source : base = .aggregate fields ∨
      base = .undefined ∧ fields = List.replicate types.length .undefined)
    (before : Initialized fields filled) :
    State.insertField (.product types) base index (.scalar value) =
      .ok (.aggregate (fields.set index (.scalar value))) ∧
    Initialized (fields.set index (.scalar value)) (index :: filled) := by
  constructor
  · rcases source with rfl | ⟨rfl, rfl⟩ <;>
      simp [State.insertField, State.ValueType.fields, State.required,
        List.getElem?_eq_getElem fits, arity, Bind.bind, Except.bind, Pure.pure, Except.pure]
  · exact set_initialized fields filled index value (by omega) before

/-- A chain of checked inserts keeps the field invariant at every intermediate
product. A new chain starts with the undefined fields and an empty mask. -/
theorem construct_initialized (types : List State.ValueType)
    (inserts : List (Nat × State.Scalar)) (base : State.Value)
    (fields : List State.Value) (filled : List Nat)
    (arity : fields.length = types.length)
    (bounds : ∀ update ∈ inserts, update.1 < types.length)
    (source : base = .aggregate fields ∨
      base = .undefined ∧ fields = List.replicate types.length .undefined)
    (before : Initialized fields filled) :
    ∃ value result,
      inserts.foldlM (fun product (index, field) =>
        State.insertField (.product types) product index (.scalar field)) base = .ok value ∧
      (value = .aggregate result ∨
        value = .undefined ∧ result = List.replicate types.length .undefined) ∧
      result.length = types.length ∧
      Initialized result ((inserts.map Prod.fst).reverse ++ filled) := by
  induction inserts generalizing base fields filled with
  | nil => exact ⟨base, fields, rfl, source, arity, by simpa using before⟩
  | cons update rest ih =>
    obtain ⟨index, field⟩ := update
    have bound := bounds (index, field) List.mem_cons_self
    obtain ⟨step, initialized⟩ :=
      insert_initialized types base fields filled index field arity bound source before
    obtain ⟨value, result, run, shape, size, ready⟩ :=
      ih (.aggregate (fields.set index (.scalar field))) (fields.set index (.scalar field))
        (index :: filled) (by simpa using arity) (fun update member =>
          bounds update (List.mem_cons_of_mem _ member)) (Or.inl rfl) initialized
    refine ⟨value, result, ?_, shape, size, ?_⟩
    · simpa [List.foldlM_cons, step, Bind.bind, Except.bind] using run
    · simpa [List.reverse_cons, List.append_assoc] using ready

theorem observed_initialized (inserts : List Insert) (reads : List Nat)
    (checked : safe inserts reads = true) (facts : Facts)
    (found : inserts.foldlM record [] = some facts)
    (id : Nat) (used : id ∈ reads) (mask : Fields)
    (lookup : facts.lookup id = some mask) (fields : List State.Value)
    (initialized : Initialized fields mask.filled) (i : Nat) (bound : i < mask.arity) :
    ∃ value, fields[i]? = some (State.Value.scalar value) := by
  have all := List.all_eq_true.mp (by simpa only [safe, found] using checked) id used
  have full : complete mask = true := by simpa only [lookup, Option.elim_some] using all
  have included := List.all_eq_true.mp full i (List.mem_range.mpr bound)
  exact initialized i (List.contains_iff_mem.mp included)

end SolarTV.MIR.ProductInit
