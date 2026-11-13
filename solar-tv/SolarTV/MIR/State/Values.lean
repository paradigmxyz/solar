import SolarTV.MIR.Ops

/-! Logical typed SSA values. Products and slices are immutable trees, not memory
objects or backend word layouts. Undefined fields stay undefined until replaced;
projecting another field does not read them. An undefined scalar cannot determine
control flow, an effect or a returned observable. At the selected function boundary
only, products are flattened in declaration order to compare type-lowering passes.
Type depth/node limits are importer resource limits, never execution assumptions. -/

namespace SolarTV.MIR.State
open EvmRules

structure Scalar where
  width : Nat
  bits : BitVec width

def wordBits (width : Nat) (value : Word) : BitVec width :=
  if h : width = 256 then value.cast h.symm else value.setWidth width

def widenBits {width : Nat} (value : BitVec width) : Word :=
  if h : width = 256 then value.cast h else value.setWidth 256

def Scalar.word (value : Scalar) : Word := widenBits value.bits

def Scalar.ofWord (width : Nat) (value : Word) : Scalar := ⟨width,wordBits width value⟩

/-- Full-width boxing is identity, including before dependent casts are unfolded. -/
@[simp↓] theorem scalar_word_full (word : Word) :
    (Scalar.mk 256 word).word = word := by rfl

@[simp↓] theorem wordBits_full (word : Word) : wordBits 256 word = word := by rfl

@[simp↓] theorem widenBits_full (word : Word) : widenBits word = word := by rfl

inductive ValueType where
  | word (width : Nat)
  | pointer (location : Nat)
  | product (fields : List ValueType)
  | slice (location : Nat)

mutual
  def ValueType.same (a b : ValueType) : Bool :=
    match a,b with
    | .word a,.word b | .pointer a,.pointer b | .slice a,.slice b => a == b
    | .product a,.product b => sameTypes a b
    | _,_ => false
  termination_by sizeOf a

  def sameTypes (a b : List ValueType) : Bool :=
    match a,b with
    | [],[] => true
    | a :: rest,b :: tail => a.same b && sameTypes rest tail
    | _,_ => false
  termination_by sizeOf a
end


def ValueType.width : ValueType → Nat
  | .word width => width
  | .pointer _ => 256
  | _ => 0

def ValueType.fields : ValueType → Option (List ValueType)
  | .product fields => some fields
  | .slice location => some [.pointer location,.word 256]
  | _ => none

inductive Value where
  | scalar (value : Scalar)
  | aggregate (fields : List Value)
  | undefined

/-- Undefined components may be copied, but never silently materialized as zero. -/
def Value.fits : Nat → ValueType → Value → Bool
  | 0,_,_ => false
  | _ + 1,_,.undefined => true
  | _ + 1,ty,.scalar value => ty.width > 0 && ty.width ≤ 256 && ty.width == value.width
  | fuel + 1,ty,.aggregate values =>
    match ty.fields with
    | none => false
    | some types => types.length == values.length &&
      (types.zip values).all (fun (ty,value) => value.fits fuel ty)

/-- Scalars alone may feed word operations. Pointer bits carry no validity proof. -/
def Value.word : Value → Except String Word
  | .scalar value => .ok value.word
  | .undefined => .error "undefined MIR scalar operand"
  | _ => .error "aggregate MIR operand used as a scalar"

/-- Only complete, defined logical leaves are exposed at the function boundary. -/
def Value.flatten : Nat → ValueType → Value → Except String (List Word)
  | 0,_,_ => .error "MIR aggregate type depth limit"
  | fuel + 1,ty,value => do
    match ty.fields with
    | none => return [← value.word]
    | some types =>
      let values ← match value with
        | .undefined => pure (List.replicate types.length Value.undefined)
        | .aggregate values => pure values
        | _ => .error "MIR aggregate return shape mismatch"
      if types.length != values.length then .error "MIR aggregate return arity" else
        return (← (types.zip values).mapM (fun (ty,value) => value.flatten fuel ty)).flatten

/-- Normalize value conditionals without distributing the whole execution state. -/
@[simp↓] theorem fits_ite (fuel : Nat) (ty : ValueType) (p : Prop) [Decidable p] (a b : Value) :
    (if p then a else b).fits fuel ty = (if p then a.fits fuel ty else b.fits fuel ty) := by
  split <;> rfl

@[simp↓] theorem word_ite (p : Prop) [Decidable p] (a b : Value) :
    (if p then a else b).word = (if p then a.word else b.word) := by
  split <;> rfl

@[simp↓] theorem flatten_ite (fuel : Nat) (ty : ValueType) (p : Prop) [Decidable p] (a b : Value) :
    (if p then a else b).flatten fuel ty = (if p then a.flatten fuel ty else b.flatten fuel ty) := by
  split <;> rfl

@[simp↓] theorem except_ok_ite (p : Prop) [Decidable p] (a b : α) :
    (if p then Except.ok a else Except.ok b : Except ε α) = .ok (if p then a else b) := by
  split <;> rfl

end SolarTV.MIR.State
