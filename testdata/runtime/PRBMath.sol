// SPDX-License-Identifier: MIT
pragma solidity >=0.8.19;

import {exp2, mulDiv, mulDiv18, sqrt} from "src/Common.sol";
import {SD59x18} from "src/SD59x18.sol";
import {UD60x18} from "src/UD60x18.sol";

// Plain external wrappers over the fixed-point math a DeFi caller uses, with
// raw words in and out so each call measures the library and the ABI only.
contract PRBMath {
    function commonExp2(uint256 x) external pure returns (uint256) {
        return exp2(x);
    }

    function commonMulDiv(uint256 x, uint256 y, uint256 denominator) external pure returns (uint256) {
        return mulDiv(x, y, denominator);
    }

    function commonMulDiv18(uint256 x, uint256 y) external pure returns (uint256) {
        return mulDiv18(x, y);
    }

    function commonSqrt(uint256 x) external pure returns (uint256) {
        return sqrt(x);
    }

    function udExp(uint256 x) external pure returns (uint256) {
        return UD60x18.wrap(x).exp().unwrap();
    }

    function udExp2(uint256 x) external pure returns (uint256) {
        return UD60x18.wrap(x).exp2().unwrap();
    }

    function udLog2(uint256 x) external pure returns (uint256) {
        return UD60x18.wrap(x).log2().unwrap();
    }

    function udLn(uint256 x) external pure returns (uint256) {
        return UD60x18.wrap(x).ln().unwrap();
    }

    function udSqrt(uint256 x) external pure returns (uint256) {
        return UD60x18.wrap(x).sqrt().unwrap();
    }

    function udPow(uint256 x, uint256 y) external pure returns (uint256) {
        return UD60x18.wrap(x).pow(UD60x18.wrap(y)).unwrap();
    }

    function udMul(uint256 x, uint256 y) external pure returns (uint256) {
        return UD60x18.wrap(x).mul(UD60x18.wrap(y)).unwrap();
    }

    function udDiv(uint256 x, uint256 y) external pure returns (uint256) {
        return UD60x18.wrap(x).div(UD60x18.wrap(y)).unwrap();
    }

    function sdExp(int256 x) external pure returns (int256) {
        return SD59x18.wrap(x).exp().unwrap();
    }

    function sdExp2(int256 x) external pure returns (int256) {
        return SD59x18.wrap(x).exp2().unwrap();
    }

    function sdLog2(int256 x) external pure returns (int256) {
        return SD59x18.wrap(x).log2().unwrap();
    }

    function sdLn(int256 x) external pure returns (int256) {
        return SD59x18.wrap(x).ln().unwrap();
    }

    function sdSqrt(int256 x) external pure returns (int256) {
        return SD59x18.wrap(x).sqrt().unwrap();
    }

    function sdPow(int256 x, int256 y) external pure returns (int256) {
        return SD59x18.wrap(x).pow(SD59x18.wrap(y)).unwrap();
    }

    function sdMul(int256 x, int256 y) external pure returns (int256) {
        return SD59x18.wrap(x).mul(SD59x18.wrap(y)).unwrap();
    }

    function sdDiv(int256 x, int256 y) external pure returns (int256) {
        return SD59x18.wrap(x).div(SD59x18.wrap(y)).unwrap();
    }
}
