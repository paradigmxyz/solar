import SolarTV.MIR.State.IR

/-! Functional product updates and projections. Construction has no memory effect.
An undefined product provides undefined components, rather than zero-filled words;
a projection reads only its selected component. Location-tagged slices use the
same product algebra, with exactly a pointer and a word length. -/

namespace SolarTV.MIR.State

def insertField (ty : ValueType) (aggregate : Value) (index : Nat) (value : Value) : Except String Value := do
  let types ← required ty.fields "MIR insertion needs a product"
  let _ ← required types[index]? "MIR aggregate index"
  let fields ← match aggregate with
    | .undefined => pure (List.replicate types.length Value.undefined)
    | .aggregate fields => pure fields
    | _ => .error "MIR insertion needs a product"
  if fields.length != types.length then .error "MIR aggregate arity mismatch" else
    return .aggregate (fields.set index value)

def extractField (ty : ValueType) (aggregate : Value) (index : Nat) : Except String Value := do
  let types ← required ty.fields "MIR projection needs a product"
  let _ ← required types[index]? "MIR aggregate index"
  match aggregate with
  | .undefined => pure Value.undefined
  | .aggregate fields => required fields[index]? "MIR aggregate index"
  | _ => .error "MIR projection needs a product"

@[simp↓] theorem insertField_ite (ty : ValueType) (index : Nat) (value : Value)
    (p : Prop) [Decidable p] (a b : Value) :
    insertField ty (if p then a else b) index value =
      (if p then insertField ty a index value else insertField ty b index value) := by
  split <;> rfl

@[simp↓] theorem extractField_ite (ty : ValueType) (index : Nat)
    (p : Prop) [Decidable p] (a b : Value) :
    extractField ty (if p then a else b) index =
      (if p then extractField ty a index else extractField ty b index) := by
  split <;> rfl

/-- Reading an initialized field does not consult the still-undefined neighbor. -/
theorem project_inserted_field (word : EvmRules.Word) :
    (do let value ← insertField (.product [.word 256,.word 1]) .undefined 0
          (.scalar (Scalar.ofWord 256 word))
        extractField (.product [.word 256,.word 1]) value 0) =
      .ok (.scalar (Scalar.ofWord 256 word)) := by rfl

/-- The uninitialized neighbor has not acquired an arbitrary value. -/
theorem uninitialized_neighbor :
    (do let value ← insertField (.product [.word 256,.word 1]) .undefined 0
          (.scalar (Scalar.ofWord 256 7))
        let field ← extractField (.product [.word 256,.word 1]) value 1
        field.word) = .error "undefined MIR scalar operand" := by rfl

end SolarTV.MIR.State
