# Uniswap v2-core specs for solar-verify.
#
# Applied to the vendored ^0.8 port of v2-core by apply_specs.py, which writes
# a copy with each block below added as NatSpec in front of the named contract,
# function or statement; the port itself is not edited. See apply_specs.py for
# the format. No spec here is a loop invariant: solar-verify infers those.
#
# Statement targets carry only ghost updates (@custom:ghost), which record a
# local value for the postconditions; ghost code is checked not to interfere
# with real execution.

# The pair is locked while a lock-guarded function runs.
@define LOCKED unlocked != 1
@define SUPPLY_KEPT totalSupply == old(totalSupply)
@define BALANCES_KEPT forall address a :: balanceOf[a] == old(balanceOf[a])
@define ALLOWANCES_KEPT forall address o, address s :: allowance[o][s] == old(allowance[o][s])
@define RESERVES_KEPT reserve0 == old(reserve0) && reserve1 == old(reserve1)
# Amount of a token that came in during a swap: the new balance (the new
# reserve) minus what the old reserve keeps after the amount sent out.
@define IN(r, out) (r > old(r) - out ? r - (old(r) - out) : 0)

# ---------------------------------------------------------------- ERC20

== contract UniswapV2ERC20
@custom:ghost uint256 sumBalances
# The supply is the sum of all balances.
@custom:invariant totalSupply == sumBalances
# Nobody can spend the zero address's tokens: it never signs and is never a
# caller, so it never grants an allowance.
@custom:invariant forall address s :: allowance[address(0)][s] == 0
# Tokens minted to the zero address stay there: zeroMinted counts them.
@custom:ghost uint256 zeroMinted
@custom:invariant balanceOf[address(0)] >= zeroMinted

== statement UniswapV2ERC20._mint /balanceOf\[to\] = /
@custom:ghost sumBalances = sumBalances + value
@custom:ghost zeroMinted = to == address(0) ? zeroMinted + value : zeroMinted

== statement UniswapV2ERC20._burn /totalSupply = /
@custom:ghost sumBalances = sumBalances - value

== function UniswapV2ERC20.transfer
@custom:ensures msg.sender != to ==> balanceOf[msg.sender] == old(balanceOf[msg.sender]) - value
@custom:ensures msg.sender != to ==> balanceOf[to] == old(balanceOf[to]) + value
@custom:ensures forall address a :: a != msg.sender && a != to ==> balanceOf[a] == old(balanceOf[a])
@custom:ensures SUPPLY_KEPT
@custom:ensures ALLOWANCES_KEPT
@custom:ensures result

== function UniswapV2ERC20.transferFrom
@custom:ensures from != to ==> balanceOf[from] == old(balanceOf[from]) - value
@custom:ensures from != to ==> balanceOf[to] == old(balanceOf[to]) + value
@custom:ensures forall address a :: a != from && a != to ==> balanceOf[a] == old(balanceOf[a])
# An infinite allowance stays infinite; any other is spent by value.
@custom:ensures old(allowance[from][msg.sender]) == type(uint256).max ==> allowance[from][msg.sender] == type(uint256).max
@custom:ensures old(allowance[from][msg.sender]) != type(uint256).max ==> allowance[from][msg.sender] == old(allowance[from][msg.sender]) - value
@custom:ensures forall address o, address s :: (o != from || s != msg.sender) ==> allowance[o][s] == old(allowance[o][s])
@custom:ensures SUPPLY_KEPT
@custom:ensures result

== function UniswapV2ERC20.approve
@custom:ensures allowance[msg.sender][spender] == value
@custom:ensures forall address o, address s :: (o != msg.sender || s != spender) ==> allowance[o][s] == old(allowance[o][s])
@custom:ensures BALANCES_KEPT
@custom:ensures SUPPLY_KEPT
@custom:ensures result

== function UniswapV2ERC20.permit
@custom:ensures owner != address(0)
@custom:ensures allowance[owner][spender] == value
@custom:ensures nonces[owner] == old(nonces[owner]) + 1
@custom:ensures forall address o, address s :: (o != owner || s != spender) ==> allowance[o][s] == old(allowance[o][s])
@custom:ensures BALANCES_KEPT
@custom:ensures SUPPLY_KEPT

# ---------------------------------------------------------------- Math

== function Math.sqrt
@custom:ensures z * z <= y && y < (z + 1) * (z + 1)

== function Math.min
@custom:ensures z <= x && z <= y && (z == x || z == y)

# ---------------------------------------------------------------- Pair

== contract UniswapV2Pair
@custom:reentrancy-guard LOCKED
@custom:ghost uint256 mintAmount0
@custom:ghost uint256 mintAmount1
@custom:ghost uint256 mintSupply
@custom:ghost uint256 burnLiquidity
@custom:ghost uint256 burnBalance0
@custom:ghost uint256 burnBalance1
@custom:ghost uint256 burnSupply

== function UniswapV2Pair._update
# Reserves equal the balances (which must fit in uint112) and the timestamp
# is the block's, mod 2^32.
@custom:ensures reserve0 == balance0 && reserve1 == balance1
@custom:ensures blockTimestampLast == block.timestamp % 2**32

== function UniswapV2Pair.initialize
@custom:ensures old(factory) == msg.sender
@custom:ensures token0 == _token0 && token1 == _token1

== statement UniswapV2Pair.mint /if \(_totalSupply == 0\)/
@custom:ghost mintAmount0 = amount0
@custom:ghost mintAmount1 = amount1
@custom:ghost mintSupply = _totalSupply

== function UniswapV2Pair.mint
# First mint: liquidity is sqrt(amount0 * amount1) - MINIMUM_LIQUIDITY, and
# MINIMUM_LIQUIDITY goes to the zero address.
@custom:ensures mintSupply == 0 ==> (liquidity + 1000) * (liquidity + 1000) <= mintAmount0 * mintAmount1 && mintAmount0 * mintAmount1 < (liquidity + 1001) * (liquidity + 1001)
@custom:ensures mintSupply == 0 && to != address(0) ==> balanceOf[address(0)] == old(balanceOf[address(0)]) + 1000
# ... where they stay for good (see zeroMinted in UniswapV2ERC20).
@custom:ensures mintSupply == 0 ==> zeroMinted >= old(zeroMinted) + 1000
# Later mints: liquidity is the largest amount proportional to both deposits,
# floor(min(amount0 * supply / reserve0, amount1 * supply / reserve1)).
@custom:ensures mintSupply != 0 ==> liquidity * old(reserve0) <= mintAmount0 * mintSupply && liquidity * old(reserve1) <= mintAmount1 * mintSupply
@custom:ensures mintSupply != 0 ==> (liquidity + 1) * old(reserve0) > mintAmount0 * mintSupply || (liquidity + 1) * old(reserve1) > mintAmount1 * mintSupply
@custom:ensures totalSupply == mintSupply + liquidity + (mintSupply == 0 ? 1000 : 0)
@custom:ensures liquidity > 0
@custom:ensures reserve0 == old(reserve0) + mintAmount0 && reserve1 == old(reserve1) + mintAmount1
@custom:ensures !(LOCKED)

== statement UniswapV2Pair.burn /amount0 = liquidity/
@custom:ghost burnLiquidity = liquidity
@custom:ghost burnBalance0 = balance0
@custom:ghost burnBalance1 = balance1
@custom:ghost burnSupply = _totalSupply

== function UniswapV2Pair.burn
# Each amount is the burned share of the balance, rounded down.
@custom:ensures amount0 * burnSupply <= burnLiquidity * burnBalance0 && burnLiquidity * burnBalance0 < (amount0 + 1) * burnSupply
@custom:ensures amount1 * burnSupply <= burnLiquidity * burnBalance1 && burnLiquidity * burnBalance1 < (amount1 + 1) * burnSupply
@custom:ensures amount0 > 0 && amount1 > 0
@custom:ensures totalSupply == burnSupply - burnLiquidity
@custom:ensures !(LOCKED)

== function UniswapV2Pair.swap
# Constant product with the 0.3% fee: the fee-adjusted new balances keep
# reserve0 * reserve1 * 1000^2, and so k never decreases.
@custom:ensures (reserve0 * 1000 - IN(reserve0, amount0Out) * 3) * (reserve1 * 1000 - IN(reserve1, amount1Out) * 3) >= old(reserve0) * old(reserve1) * 1000**2
@custom:ensures reserve0 * reserve1 >= old(reserve0) * old(reserve1)
@custom:ensures IN(reserve0, amount0Out) > 0 || IN(reserve1, amount1Out) > 0
@custom:ensures amount0Out < old(reserve0) && amount1Out < old(reserve1)
@custom:ensures SUPPLY_KEPT
@custom:ensures !(LOCKED)

== function UniswapV2Pair.skim
@custom:ensures RESERVES_KEPT
@custom:ensures SUPPLY_KEPT
@custom:ensures !(LOCKED)

== function UniswapV2Pair.sync
@custom:ensures SUPPLY_KEPT
@custom:ensures !(LOCKED)

# ---------------------------------------------------------------- Factory

== contract UniswapV2Factory
# getPair is symmetric, never pairs a token with itself, and maps each pair
# address to one unordered pair of tokens.
@custom:invariant forall address a, address b :: getPair[a][b] == getPair[b][a]
@custom:invariant forall address a :: getPair[a][a] == address(0)
# Every pair recorded is a contract this factory created (__created is
# solar-verify's ghost set of addresses the contract deployed).
@custom:invariant forall address x, address y :: getPair[x][y] != address(0) ==> __created[getPair[x][y]]
@custom:invariant forall address a, address b, address c, address d :: getPair[a][b] != address(0) && getPair[a][b] == getPair[c][d] ==> (a == c && b == d) || (a == d && b == c)

== function UniswapV2Factory.createPair
@custom:ensures pair != address(0)
@custom:ensures getPair[tokenA][tokenB] == pair && getPair[tokenB][tokenA] == pair
@custom:ensures old(getPair[tokenA][tokenB]) == address(0)
@custom:ensures allPairs.length == old(allPairs.length) + 1 && allPairs[allPairs.length - 1] == pair

== function UniswapV2Factory.setFeeTo
@custom:ensures old(feeToSetter) == msg.sender && feeTo == _feeTo

== function UniswapV2Factory.setFeeToSetter
@custom:ensures old(feeToSetter) == msg.sender && feeToSetter == _feeToSetter
