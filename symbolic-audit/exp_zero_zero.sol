// `0 ** 0` is 1 in Solidity (and EVM `EXP`). solar returns 0 on 9bc465922;
// earlier builds returned 1.
// Source: testdata/solidity/test/libsolidity/semanticTests/expressions/exp_zero_literal.sol
contract ExpZeroZero {
    function literal() external pure returns (uint256) {
        return 0 ** 0;
    }

    function typedBase() external pure returns (uint256) {
        uint256 b = 0;
        return b ** 0;
    }

    function typedExp() external pure returns (uint256) {
        uint256 e = 0;
        return 0 ** e;
    }

    function runtime(uint256 b, uint256 e) external pure returns (uint256) {
        return b ** e;
    }

    function runtimeUnchecked(uint256 b, uint256 e) external pure returns (uint256) {
        unchecked {
            return b ** e;
        }
    }

    function zeroBase(uint256 e) external pure returns (uint256) {
        return 0 ** e;
    }

    function zeroExp(uint256 b) external pure returns (uint256) {
        return b ** 0;
    }

    function oneBase(uint256 e) external pure returns (uint256) {
        return 1 ** e;
    }

    function twoZero() external pure returns (uint256) {
        return 2 ** 0;
    }

    function narrow(uint8 b, uint8 e) external pure returns (uint8) {
        return b ** e;
    }

    function signedZero(int256 b) external pure returns (int256) {
        return b ** 0;
    }
}
