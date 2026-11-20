# UniswapV2ERC20.transfer, the first certified slice (solidity-lean docs/CERTIFICATION_TARGET.md).
#
# Applied by apply_specs.py like uniswap-v2.spec, on its own: no contract invariant, no ghost and
# no clause quantified over addresses, so the exported spec reads only the two balance slots the
# call touches, balanceOf[msg.sender] and balanceOf[to], and needs keccak256 apart on those two
# preimages only. The source's storage updates are sequential: the debit of the caller, then the
# credit of the recipient reading the debited state. So when msg.sender == to the credit cannot
# overflow and the balance ends unchanged; no recipient-overflow condition applies.
#
# What the spec language cannot state (the Transfer log, the revert data, the frame over the
# slots the call does not write) is stated over the observable in solidity-lean
# (Contracts/UniswapV2/ERC20/Slice/Post.lean), next to this spec's export.

== function UniswapV2ERC20.transfer
@custom:succeeds-if balanceOf[msg.sender] >= value && (msg.sender == to || balanceOf[to] <= type(uint256).max - value)
@custom:ensures old(balanceOf[msg.sender]) >= value
@custom:ensures msg.sender != to ==> old(balanceOf[to]) <= type(uint256).max - value
@custom:ensures msg.sender != to ==> balanceOf[msg.sender] == old(balanceOf[msg.sender]) - value
@custom:ensures msg.sender != to ==> balanceOf[to] == old(balanceOf[to]) + value
@custom:ensures msg.sender == to ==> balanceOf[msg.sender] == old(balanceOf[msg.sender])
@custom:ensures result
