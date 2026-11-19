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

/-- The emitted ABI decoders use aligned, word-sized stores. Track their
active memory in words while keeping ordinary byte memory in each EVM step. -/
def wordArena (words : List Word) : List Byte := words.flatMap wordBytes

theorem wordArena_cons (head : Word) (tail : List Word) :
    wordArena (head :: tail) = wordBytes head ++ wordArena tail := rfl

theorem wordArena_append (left right : List Word) :
    wordArena (left ++ right) = wordArena left ++ wordArena right := by
  simp [wordArena,List.flatMap_append]

@[simp] theorem wordArena_length (words : List Word) :
    (wordArena words).length = words.length * 32 := by
  induction words with
  | nil => rfl
  | cons head tail ih =>
    simp only [wordArena_cons,List.length_append,wordBytes_length,ih,List.length_cons]
    omega

private theorem zeros (n : Nat) : wordArena (List.replicate n 0) = List.replicate (32*n) 0 := by
  have wordZero : wordBytes (0 : Word) = List.replicate 32 0 := by decide
  induction n with
  | zero => rfl
  | succ n ih =>
    rw [List.replicate_succ,wordArena_cons,wordZero,ih, List.replicate_append_replicate]
    congr 1
    omega

private theorem takeArena (words : List Word) (n : Nat) :
    (wordArena words).take (32*n) = wordArena (words.take n) := by
  induction words generalizing n with
  | nil => simp [wordArena]
  | cons word words ih =>
    cases n with
    | zero => rfl
    | succ n =>
      have count : 32 * (n+1) = (wordBytes word).length + 32*n := by
        simp [Nat.mul_succ,Nat.add_comm]
      rw [count]
      simp only [wordArena_cons,List.take_succ_cons,List.take_append,
        List.take_of_length_le (Nat.le_add_right _ _),Nat.add_sub_cancel_left]
      exact congrArg (wordBytes word ++ ·) (ih n)

theorem dropArena (words : List Word) (n : Nat) :
    (wordArena words).drop (32*n) = wordArena (words.drop n) := by
  induction words generalizing n with
  | nil => simp [wordArena]
  | cons word words ih =>
    cases n with
    | zero => rfl
    | succ n =>
      have count : 32 * (n+1) = (wordBytes word).length + 32*n := by
        simp [Nat.mul_succ,Nat.add_comm]
      rw [count]
      simp only [wordArena_cons,List.drop_succ_cons,List.drop_append,
        List.drop_of_length_le (Nat.le_add_right _ _),List.nil_append,
        Nat.add_sub_cancel_left,ih]

theorem readArena (words : List Word) (index count : Nat)
    (within : index + count ≤ words.length) :
    readBytes (wordArena words) (32*index) (32*count) =
      wordArena ((words.drop index).take count) := by
  have read : readBytes (wordArena words) (32*index) (32*count) =
      ((wordArena words).drop (32*index)).take (32*count) := by
    apply List.ext_getElem
    · simp [readBytes,wordArena_length]
      omega
    · intro i h _
      have present : 32*index+i < (wordArena words).length := by
        simp only [readBytes,List.length_map,List.length_range] at h
        rw [wordArena_length]
        omega
      have present' : i+32*index < (wordArena words).length := by omega
      simp [readBytes,Nat.add_comm,List.getElem?_eq_getElem present']
  rw [read,dropArena,takeArena]

theorem arenaRange (words : List Word) (index count : Nat)
    (within : index + count ≤ words.length) :
    requiredWords (wordArena words) (32*index) (32*count) = words.length := by
  by_cases zero : count = 0
  · simp [zero,requiredWords,memoryWords,Nat.mul_comm words.length]
  · unfold requiredWords memoryWords
    simp only [wordArena_length,show 32*count ≠ 0 by omega,ite_false]
    omega

theorem expandArenaRange (words : List Word) (index count : Nat)
    (within : index + count ≤ words.length) (bounded : 32 * words.length ≤ 65536) :
    expand (wordArena words) (32*index) (32*count) = .ok (wordArena words) := by
  simp [expand,arenaRange words index count within,Nat.not_lt_of_ge bounded,
    Nat.mul_comm words.length]

def activeWords (words : List Word) (index : Nat) : List Word :=
  words ++ List.replicate (index+1-words.length) 0

def storeArena (words : List Word) (index : Nat) (value : Word) : List Word :=
  let active := activeWords words index
  active.take index ++ value :: active.drop (index+1)

@[simp] theorem requiredWordStore (words : List Word) (index : Nat) :
    requiredWords (wordArena words) (32*index) 32 = max words.length (index+1) := by
  unfold requiredWords memoryWords
  simp only [wordArena_length,show ¬ (32 : Nat) = 0 by decide,ite_false]
  congr 1 <;> omega

theorem expandArena (words : List Word) (index : Nat)
    (bounds : 32 * max words.length (index+1) ≤ 65536) :
    expand (wordArena words) (32*index) 32 = .ok (wordArena (activeWords words index)) := by
  simp only [expand,requiredWordStore,Nat.not_lt_of_ge bounds,ite_false,wordArena_length]
  have growth : 32 * max words.length (index+1) - words.length * 32 =
      32 * (index+1-words.length) := by omega
  rw [growth]
  rw [activeWords,wordArena_append,zeros]

theorem writeArena (words : List Word) (index : Nat) (value : Word) :
    writeBytes (wordArena (activeWords words index)) (32*index) (wordBytes value) =
      wordArena (storeArena words index value) := by
  have bound : 32 ≤ (wordArena (activeWords words index)).length - 32*index := by
    simp only [wordArena_length,activeWords,List.length_append,List.length_replicate]
    omega
  unfold writeBytes
  rw [takeArena,List.take_of_length_le (l := wordBytes value) (by simpa using bound),
    show 32 * index + (wordBytes value).length = 32 * (index+1) by simp [Nat.mul_succ],
    dropArena]
  simp only [storeArena,wordArena_append,wordArena_cons,List.append_assoc]

theorem writeActiveArena (words : List Word) (index : Nat) (value : Word)
    (within : index < words.length) :
    writeBytes (wordArena words) (32*index) (wordBytes value) =
      wordArena (storeArena words index value) := by
  have active : activeWords words index = words := by
    simp [activeWords,show index+1-words.length = 0 by omega]
  simpa only [active] using writeArena words index value

theorem marketEntryFrame (caller : Caller) (journal : Journal) (a b c d e : Word)
    (parameters : List (Nat × Word)) (start length : Nat) (market : hasMarket caller = true) :
    entryFrame caller journal [a,b,c,d,e] parameters start length =
      .ok ⟨journal,wordArena [0,0,288,0,a,b,c,d,e],
        scalar 256 128 :: parameters.map (fun (width,word) => scalar width word) ++
          [.aggregate [scalar 256 (BitVec.ofNat 256 start),scalar 256 (BitVec.ofNat 256 length)]],
        [],[],0,0,[]⟩ := by
  unfold entryFrame
  simp only [market,ite_true]
  rw [show expand [] 0 288 = .ok (wordArena [0,0,0,0,0,0,0,0,0]) by
    change (Except.ok (List.replicate (32*9) 0) : Except String (List Byte)) = _
    rw [← zeros]
    rfl]
  change Except.ok (⟨journal,
    writeBytes (writeBytes (writeBytes (writeBytes (writeBytes
      (writeBytes (wordArena [0,0,0,0,0,0,0,0,0]) (32*2) (wordBytes 288))
      (32*4) (wordBytes a)) (32*5) (wordBytes b)) (32*6) (wordBytes c))
      (32*7) (wordBytes d)) (32*8) (wordBytes e),
    _,[],[],0,0,[]⟩ : MIR.State.Frame) = _
  rw [writeActiveArena _ 2 _ (by decide)]
  rw [writeActiveArena _ 4 _ (by decide)]
  rw [writeActiveArena _ 5 _ (by change 5 < 9; decide)]
  rw [writeActiveArena _ 6 _ (by change 6 < 9; decide)]
  rw [writeActiveArena _ 7 _ (by change 7 < 9; decide)]
  rw [writeActiveArena _ 8 _ (by change 8 < 9; decide)]
  rfl

theorem marketEntryArena (caller : Caller) (journal : Journal) (a b c d e : Word)
    (parameters : List (Nat × Word)) (start length : Nat) (market : hasMarket caller = true) :
    (entryFrame caller journal [a,b,c,d,e] parameters start length).map (·.memory) =
      .ok (wordArena [0,0,288,0,a,b,c,d,e]) := by
  rw [marketEntryFrame caller journal a b c d e parameters start length market]
  rfl

theorem flashEntryArena (journal : Journal) (parameters : List (Nat × Word)) (start length : Nat) :
    (entryFrame .flashLoan journal [] parameters start length).map (·.memory) =
      .ok (wordArena [0,0,128]) := by
  change Except.ok (writeBytes (wordArena [0,0,0]) (32*2) (wordBytes 128)) = _
  rw [writeActiveArena _ 2 _ (by decide)]
  rfl

theorem wordSize (size : Nat) (small : size < 2^256) :
    (BitVec.ofNat 256 size).toNat = size := by simp [Nat.mod_eq_of_lt small]

theorem sizeSub (size n : Nat) (small : size < 2^256) (bounds : n ≤ size) :
    Evm.sub (BitVec.ofNat 256 size) (BitVec.ofNat 256 n) = BitVec.ofNat 256 (size-n) := by
  unfold Evm.sub
  rw [BitVec.ofNat_sub_ofNat_of_le _ _ (by omega) bounds]

theorem ltSize (size n : Nat) (small : size < 2^256) (bounds : n ≤ size) :
    Evm.lt (BitVec.ofNat 256 size) (BitVec.ofNat 256 n) = 0 := by
  simp [Evm.lt,BitVec.ult_eq_decide,wordSize _ small,wordSize n (by omega),Nat.not_lt_of_ge bounds]

theorem addressShift (word : Word) (canonical : word.toNat < 2^160) :
    Evm.shr 160 word = 0 := by
  change word >>> 160 = 0
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_ushiftRight,Nat.shiftRight_eq_div_pow,Nat.div_eq_of_lt canonical]
  rfl

theorem fitsSlice (word : Word) (size : Nat) (small : size < 2^256)
    (bounds : word.toNat ≤ size) : Evm.gt word (BitVec.ofNat 256 size) = 0 := by
  simp [Evm.gt,BitVec.ult_eq_decide,wordSize _ small,Nat.not_lt_of_ge bounds]

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
