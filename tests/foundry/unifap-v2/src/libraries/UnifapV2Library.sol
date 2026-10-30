// SPDX-License-Identifier: MIT
pragma solidity >=0.8.10;

import "../interfaces/IUnifapV2Pair.sol";
import "../interfaces/IUnifapV2Factory.sol";

/// @title UnifapV2Library
/// @author Uniswap Labs
/// @notice Provides common functionality for UnifapV2 Contracts
library UnifapV2Library {
    function sortPairs(address token0, address token1)
        internal
        pure
        returns (address, address)
    {
        return token0 < token1 ? (token0, token1) : (token1, token0);
    }

    function quote(
        uint256 amount0,
        uint256 reserve0,
        uint256 reserve1
    ) internal pure returns (uint256) {
        return (amount0 * reserve1) / reserve0;
    }

    function getReserves(
        address factory,
        address tokenA,
        address tokenB
    ) internal view returns (uint112 reserveA, uint112 reserveB) {
        (address token0, address token1) = sortPairs(tokenA, tokenB);
        (uint112 reserve0, uint112 reserve1, ) = IUnifapV2Pair(
            IUnifapV2Factory(factory).pairs(token0, token1)
        ).getReserves();
        (reserveA, reserveB) = tokenA == token0
            ? (reserve0, reserve1)
            : (reserve1, reserve0);
    }

    // Query the factory for the pair address.
    function pairFor(
        address factory,
        address tokenA,
        address tokenB
    ) internal view returns (address pair) {
        pair = IUnifapV2Factory(factory).pairs(tokenA, tokenB);
    }
}
