import SolarTV.EVMIR.State.Decode

/-! SELFDESTRUCT's immediate balance effects and deferred account deletion.
Code and storage survive until transaction finalization, allowing another call
to the same account before the root returns. Cancun preserves existing accounts
under EIP-6780; accounts created in this transaction retain the legacy deletion
and self-beneficiary balance-burning rules. The flags are journaled with accounts,
so reverting any parent also reverts deletion, creation and refund eligibility.
https://eips.ethereum.org/EIPS/eip-6780
https://eips.ethereum.org/EIPS/eip-3529
https://eips.ethereum.org/EIPS/eip-2929 -/

namespace SolarTV.EVM.State

def destructionPrice (fork : Fork) (world : Journal) (owner beneficiary : Address) : Nat :=
  5000 + (if fork.atLeast .berlin && !world.warmAddresses.contains beneficiary then 2600 else 0) +
    if (account world owner).balance != 0 && dead world beneficiary then 25000 else 0

def destroy (fork : Fork) (world : Journal) (owner beneficiary : Address) : Journal :=
  let old := account world owner
  let deletes := !fork.atLeast .cancun || old.created
  let world := warmAddress world beneficiary
  let world := if owner = beneficiary then world else
    (transfer world owner beneficiary old.balance).getD world
  let current := account world owner
  let refund := if !fork.atLeast .london && !old.destroyed then 24000 else 0
  putAccount {world with refund := world.refund + refund}
    {current with
      balance := if deletes then 0 else current.balance
      destroyed := old.destroyed || deletes}

def finalize (world : Journal) : Journal :=
  {world with accounts := world.accounts.filter (fun a => !a.destroyed)}

end SolarTV.EVM.State
