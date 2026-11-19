import SolarTV.MIR.Transforms.TransferFromDispatch
/-! A symbolic ABI input for the five Morpho signatures. The executable MIR
decoder is proved to read this encoding for arbitrary head words, callback
bytes and initial journal. -/

namespace SolarTV.MIR.TransferFromABI
open EvmRules SolarTV.Execution
open MIR.TransferFromCallers MIR.TransferFromEntry
private theorem bytesAt (head tail : List Byte) (offset count : Nat) :
    readBytes (head ++ tail) (head.length+offset) count = readBytes tail offset count := by
  unfold readBytes
  apply List.map_congr_left
  intro i _
  have within : head.length ≤ head.length + offset + i := by omega
  rw [List.getElem?_append_right within]
  congr 2
  omega

private theorem readAt (head tail : List Byte) (offset : Nat) :
    readWord (head ++ tail) (head.length+offset) = readWord tail offset := by
  unfold readWord
  exact congrArg (fun bytes : List Byte => (BitVec.flattenListFast bytes).setWidth 256) (bytesAt head tail offset 32)

private theorem readExact (bytes : List Byte) : readBytes bytes 0 bytes.length = bytes := by
  apply List.ext_getElem
  · simp [readBytes]
  · intro i hs ht
    simp [readBytes, List.getElem?_eq_getElem ht]

private theorem readFront (word : Word) (tail : List Byte) :
    readWord (wordBytes word ++ tail) 0 = word := by
  rw [readWord_append_prefix _ _ _ (by simp)]
  have read : readBytes (wordBytes word) 0 32 = wordBytes word := by
    simpa only [wordBytes_length] using readExact (wordBytes word)
  unfold readWord
  calc
    _ = (BitVec.flattenListFast (wordBytes word)).setWidth 256 :=
      congrArg (fun bytes : List Byte => (BitVec.flattenListFast bytes).setWidth 256) read
    _ = _ := by rw [← BitVec.flattenList_eq_flattenListFast]; exact flatten_wordBytes word

private theorem encodedWord (words : List Word) (tail : List Byte) (index : Nat) :
    ∀ word, words[index]? = some word → readWord (words.flatMap wordBytes ++ tail) (index*32) = word := by
  induction words generalizing index with
  | nil => simp
  | cons head rest ih =>
    intro word present
    cases index with
    | zero =>
      simp only [List.getElem?_cons_zero, Option.some.injEq] at present
      subst word
      simpa only [List.flatMap_cons, List.append_assoc, Nat.zero_mul] using readFront head (rest.flatMap wordBytes ++ tail)
    | succ index =>
      have offset : (index+1)*32 = (wordBytes head).length + index*32 := by simp [Nat.add_mul, Nat.add_comm]
      rw [offset]
      simp only [List.flatMap_cons, List.append_assoc, readAt]
      exact ih index word present

structure Calldata where
  tag : List Byte
  market : List Word
  parameters : List Word
  payload : List Byte

def Calldata.offset (data : Calldata) : Nat :=
  (data.market.length + data.parameters.length + 1) * 32

private def Calldata.head (data : Calldata) : List Word :=
  data.market ++ data.parameters ++
    [BitVec.ofNat 256 data.offset, BitVec.ofNat 256 data.payload.length]

private def Calldata.tail (data : Calldata) : List Byte :=
  data.payload ++ List.replicate ((32 - data.payload.length % 32) % 32) 0

def Calldata.encode (data : Calldata) : List Byte :=
  data.tag ++ data.head.flatMap wordBytes ++ data.tail

private theorem Calldata.word (data : Calldata) (tag : data.tag.length = 4) (index : Nat)
    (word : Word) (present : data.head[index]? = some word) :
    readWord data.encode (4+32*index) = word := by
  unfold Calldata.encode
  rw [List.append_assoc, ← tag, readAt, Nat.mul_comm]
  exact encodedWord _ _ _ _ present

theorem decode_encoding (caller : Caller) (cfg : MIR.State.Config) (journal : Journal)
    (data : Calldata) (input : cfg.environment.input = data.encode)
    (tag : data.tag.length = 4)
    (market : data.market.length = if hasMarket caller then 5 else 0)
    (arity : data.parameters.length = (parameterWidths caller).length)
    (canonical : canonicalAddresses data.market ((parameterWidths caller).zip data.parameters) = true)
    (payloadFits : data.payload.length < 2^256) :
    decode (MIR.TransferFromDispatch.selector data.tag) caller cfg journal =
      entryFrame caller journal data.market ((parameterWidths caller).zip data.parameters)
        (4 + data.offset + 32) data.payload.length := by
  let widths := parameterWidths caller
  let base := if hasMarket caller then 164 else 4
  have marketBase : base = 4 + 32 * data.market.length := by
    split at market
    · rename_i usesMarket
      simp [base,usesMarket,market]
    · rename_i noMarket
      simp [base,noMarket,market]
  have widthsBound : widths.length ≤ 3 := by cases caller <;> decide
  have marketBound : data.market.length ≤ 5 := by split at market <;> omega
  have offsetFits : data.offset < 2^256 := by
    unfold Calldata.offset
    change data.parameters.length = widths.length at arity
    omega
  have offsetAt : base + 32 * widths.length = 4 + 32 * (data.market.length + data.parameters.length) := by
    rw [marketBase, ← arity]
    omega
  have sizes : data.encode.length = 4 + data.offset + 32 + data.tail.length := by
    have lengthWords (l : List Word) : (l.flatMap wordBytes).length = l.length * 32 := by
      induction l with
      | nil => rfl
      | cons w rest ih => simp [ih,Nat.add_mul,Nat.add_comm]
    simp [Calldata.encode, Calldata.head, Calldata.offset, tag, lengthWords, Nat.add_mul, Nat.add_assoc] <;> omega
  have enough : base + 32 * widths.length + 32 ≤ data.encode.length := by
    rw [offsetAt,sizes]
    simp [Calldata.offset,Nat.add_mul]
    omega
  have sameTag : readWord data.encode 0 >>> 224 = MIR.TransferFromDispatch.selector data.tag := by
    rw [MIR.TransferFromDispatch.selector_word, Calldata.encode, List.append_assoc,
      MIR.TransferFromDispatch.selector_append _ _ tag]
  have marketWords :
      (if hasMarket caller then (List.range 5).map (fun i => readWord data.encode (4+32*i)) else []) =
        data.market := by
    cases h : hasMarket caller with
    | false => simpa [h] using (List.eq_nil_of_length_eq_zero (by simpa [h] using market)).symm
    | true =>
      simp only [ite_true, h] at market ⊢
      apply List.ext_getElem
      · simp [market]
      · intro i lhs rhs
        simp only [List.getElem_map, List.getElem_range]
        apply data.word tag
        simp [Calldata.head, List.getElem?_append_left rhs, List.getElem?_eq_getElem rhs]
  have parameterWords :
      widths.zipIdx.map (fun (width,i) => (width,readWord data.encode (base+32*i))) =
        widths.zip data.parameters := by
    apply List.ext_getElem
    · simp [arity,widths]
    · intro i lhs rhs
      have bound : i < data.parameters.length := by simpa [widths,arity] using rhs
      simp only [List.getElem_map,List.getElem_zipIdx,List.getElem_zip,Nat.zero_add]
      congr 1
      rw [marketBase, show 4 + 32 * data.market.length + 32*i =
        4 + 32 * (data.market.length+i) by omega]
      apply data.word tag
      have past : data.market.length ≤ data.market.length + i := Nat.le_add_right _ _
      simp [Calldata.head, List.append_assoc, List.getElem?_append_right past,
        List.getElem?_append_left bound, List.getElem?_eq_getElem bound]
  have offsetWord : readWord data.encode (base + 32 * widths.length) = BitVec.ofNat 256 data.offset := by
    rw [offsetAt]
    apply data.word tag
    simp [Calldata.head, List.append_assoc]
  have lengthWord : readWord data.encode (4+data.offset) = BitVec.ofNat 256 data.payload.length := by
    rw [show 4+data.offset = 4+32*(data.market.length+data.parameters.length+1) by
      rw [Calldata.offset, Nat.mul_comm]]
    refine data.word tag (data.market.length+data.parameters.length+1) _ ?_
    simp only [Calldata.head, List.append_assoc, Nat.add_assoc]
    rw [List.getElem?_append_right (Nat.le_add_right _ _), Nat.add_sub_cancel_left,
      List.getElem?_append_right (Nat.le_add_right _ _), Nat.add_sub_cancel_left]
    rfl
  have startFits : 4 + data.offset + 32 ≤ data.encode.length := by rw [sizes]; omega
  have payloadBound : data.payload.length ≤ data.encode.length - (4 + data.offset + 32) := by
    rw [sizes]
    simp [Calldata.tail]
  unfold decode
  change _ = _
  simp [input, Nat.not_lt_of_ge enough, sameTag, marketWords, parameterWords, canonical, offsetWord,
    Nat.mod_eq_of_lt offsetFits, lengthWord, Nat.mod_eq_of_lt payloadFits, Nat.not_lt_of_ge startFits,
    Nat.not_lt_of_ge payloadBound, widths, base]

theorem start_encoding (caller : Caller) (cfg : MIR.State.Config) (journal : Journal)
    (data : Calldata) (functions : MIR.State.Functions) (entry : Nat) (priced : ByteCut)
    (input : cfg.environment.input = data.encode)
    (tag : data.tag.length = 4)
    (market : data.market.length = if hasMarket caller then 5 else 0)
    (arity : data.parameters.length = (parameterWidths caller).length)
    (canonical : canonicalAddresses data.market ((parameterWidths caller).zip data.parameters) = true)
    (payloadFits : data.payload.length < 2^256) :
    mirStart cfg functions entry (MIR.TransferFromDispatch.selector data.tag) caller journal priced = (do
      let frame ← entryFrame caller journal data.market ((parameterWidths caller).zip data.parameters)
        (4 + data.offset + 32) data.payload.length
      let started ← MIR.State.start functions entry frame.arguments frame
      return ⟨⟨{function := entry},[],started⟩,priced.samples,false,none⟩) := by
  unfold mirStart
  rw [decode_encoding caller cfg journal data input tag market arity canonical payloadFits]

end SolarTV.MIR.TransferFromABI
