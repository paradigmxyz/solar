import SolarTV.EVMIR.State.Interpreter
import SolarTV.EVMIR.Precompiles.Signatures
import SolarTV.EVMIR.State.Transactions.Settlement

/-! Admitted EIP-7702 transactions through Osaka. The outer signer is supplied
by transaction decoding. The model checks the envelope's decoded fields,
charges intrinsic gas and increments the sender nonce before processing each
signed authorization. A processed delegation is part of the execution
checkpoint and survives message failure.
https://eips.ethereum.org/EIPS/eip-7702 -/

namespace SolarTV.EVM.Transaction.SetCode
open EvmRules SolarTV.Execution EVM.State
open Precompiles.Signatures (k1n recover)

structure Authorization where
  chain : Word
  target : Address
  nonce : Nat
  parity : Byte
  r : Word
  s : Word

def rlpList (bytes : List Byte) : List Byte :=
  if bytes.length < 56 then BitVec.ofNat 8 (192+bytes.length) :: bytes
  else [0xf8, BitVec.ofNat 8 bytes.length] ++ bytes

def digest (auth : Authorization) : Word :=
  Keccak.hash (0x05 :: rlpList
    (encodedNonce auth.chain.toNat ++ [0x94] ++ addressBytes auth.target ++ encodedNonce auth.nonce))

def authority (auth : Authorization) : Option Address :=
  if auth.parity.toNat > 1 || auth.s.toNat > k1n/2 then none else
  let signature := wordBytes (digest auth) ++ wordBytes (BitVec.ofNat 256 (27+auth.parity.toNat)) ++
    wordBytes auth.r ++ wordBytes auth.s
  match recover signature with
  | [] => none
  | output => some ((readWord output 0).setWidth 160)

def eligible (fork : Fork) (account : Account) : Bool :=
  account.code.isEmpty || (delegationTarget fork account.code).isSome

def applyResolved (fork : Fork) (chain : Word) (world : Journal) (auth : Authorization)
    (resolved : Option Address) : Journal :=
  if !(auth.chain == 0 || auth.chain == chain) || auth.nonce ≥ 2^64-1 then world else
  match resolved with
  | none => world
  | some signer =>
    let world := warmAddress world signer
    let old := account world signer
    if !eligible fork old || old.nonce != auth.nonce then world else
    let refund := if old.nonce != 0 || old.balance != 0 || !old.code.isEmpty then 12500 else 0
    let code := if auth.target == 0 then [] else [0xef,0x01,0x00] ++ addressBytes auth.target
    let world := putAccount world {old with nonce := old.nonce + 1,code}
    {world with refund := world.refund + refund}

def apply (fork : Fork) (chain : Word) (world : Journal) (auth : Authorization) : Journal :=
  applyResolved fork chain world auth (authority auth)

structure Envelope where
  signer : Address
  destination : Address
  chain : Word
  nonce : Nat
  gasLimit : Nat
  maxFee : Word
  priorityFee : Word
  value : Word
  input : List Byte
  accessList : List (Address × List Word)
  authorizations : List Authorization

def tokens (input : List Byte) : Nat :=
  input.foldl (fun n byte => n + (if byte == 0 then 1 else 4)) 0

def intrinsic (tx : Envelope) : Nat :=
  21000 + 4*tokens tx.input + 2400*tx.accessList.length +
    1900*(tx.accessList.foldl (fun n row => n+row.2.length) 0) +
    25000*tx.authorizations.length

def floorGas (tx : Envelope) : Nat := 21000 + 10*tokens tx.input

structure Prepared where
  world : Journal
  executionGas : Nat
  gasPrice : Nat

def access (world : Journal) (row : Address × List Word) : Journal :=
  row.2.foldl (fun world slot => warmSlot world row.1 slot) (warmAddress world row.1)

def prepareResolved (fork : Fork) (chain : Word) (blockGas baseFee : Nat)
    (coinbase : Address) (world : Journal) (tx : Envelope)
    (resolved : List (Authorization × Option Address)) : Except String Prepared := do
  if !fork.atLeast .prague || fork.atLeast .amsterdam then
    .error "unsupported set-code transaction fork"
  else if tx.chain != chain || tx.authorizations.isEmpty ||
      tx.authorizations.any (fun auth => auth.nonce ≥ 2^64) then .error "invalid set-code envelope"
  else if tx.priorityFee.toNat > tx.maxFee.toNat || tx.maxFee.toNat < baseFee then
    .error "invalid set-code fee"
  else if tx.gasLimit > blockGas || (fork.atLeast .osaka && tx.gasLimit > 2^24) ||
      tx.gasLimit < max (intrinsic tx) (floorGas tx) then .error "invalid set-code gas limit"
  else
    let sender := account world tx.signer
    if !eligible fork sender || sender.nonce != tx.nonce || tx.nonce ≥ 2^64-1 then
      .error "invalid set-code sender"
    else if sender.balance.toNat < tx.gasLimit*tx.maxFee.toNat + tx.value.toNat then
      .error "insufficient set-code sender funds"
    else
      let price := min tx.maxFee.toNat (baseFee+tx.priorityFee.toNat)
      let world := putAccount world {sender with
        nonce := sender.nonce+1
        balance := sender.balance - BitVec.ofNat 256 (tx.gasLimit*price)}
      let world := warmAddress (warmAddress (warmAddress world tx.signer) tx.destination) coinbase
      let precompiles := ((List.range 18).map (BitVec.ofNat 160) ++ [256]).filter (precompile fork)
      let world := tx.accessList.foldl access (precompiles.foldl warmAddress world)
      let world := resolved.foldl (fun world (auth,signer) =>
        applyResolved fork chain world auth signer) world
      let world := match delegationTarget fork (account world tx.destination).code with
        | none => world
        | some delegate => warmAddress world delegate
      return ⟨world,tx.gasLimit-intrinsic tx,price⟩

def prepare (fork : Fork) (chain : Word) (blockGas baseFee : Nat) (coinbase : Address)
    (world : Journal) (tx : Envelope) : Except String Prepared :=
  prepareResolved fork chain blockGas baseFee coinbase world tx
    (tx.authorizations.map (fun auth => (auth, authority auth)))

theorem wrong_chain (fork : Fork) (chain : Word) (world : Journal) (auth : Authorization)
    (other : auth.chain ≠ 0) (different : auth.chain ≠ chain) :
    apply fork chain world auth = world := by
  have zero : auth.chain ≠ 0#256 := other
  simp [apply,applyResolved,zero,different]

theorem nonce_max (fork : Fork) (chain : Word) (world : Journal) (auth : Authorization)
    (large : 2^64-1 ≤ auth.nonce) :
    apply fork chain world auth = world := by simp [apply,applyResolved,large]

/-- Execute a prepared EVM message and restore its checkpoint on failure. -/
def execute (fuel : Nat) (cfg : Config) (tx : Envelope) (entry : Prepared) : Run := do
  let (code,_,world) := executionCode cfg.fork entry.world tx.destination
  if precompile cfg.fork tx.destination then .error "precompile transaction entry" else
  let bytes := if code.delegated && precompile cfg.fork code.address then [] else code.bytes
  let env : World.Environment := ⟨tx.destination,code.address,tx.signer,tx.signer,tx.value,
    BitVec.ofNat 256 entry.gasPrice,tx.input,false,0⟩
  let some funded := transfer world tx.signer tx.destination tx.value |
    .error "set-code message value transfer failed"
  let result ← State.run fuel cfg funded ⟨env,bytes,0,[],[],[],entry.executionGas⟩
  return if result.stop = .success then {result with world := finalize result.world} else
    {result with world, gas := if result.stop = .exception || result.stop = .outOfGas
      then 0 else result.gas}

def executeSettled (fuel : Nat) (cfg : Config) (tx : Envelope) (entry : Prepared) :
    Except String (Result × Settlement.Gas) := do
  let result ← execute fuel cfg tx entry
  let gas := Settlement.finish tx.gasLimit (floorGas tx) ⟨result.gas,0,0,0⟩ result.world.refund 0 0
  let world := Settlement.fees result.world tx.signer cfg.block.beneficiary gas
    tx.gasLimit (BitVec.ofNat 256 entry.gasPrice) cfg.block.baseFee
  return ({result with world},gas)

end SolarTV.EVM.Transaction.SetCode
