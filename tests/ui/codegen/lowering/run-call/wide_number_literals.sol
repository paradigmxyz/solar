//@ codegen-matrix: standard
//@ run-call: quotient => 10000000000000000000000000000000000000000000000000000000000000000000000000000
//@ run-call: fraction => 15000000
//@ run-call: hexLiteral => 0x1000000000000000000000000000000000000000000000000000000000000000
//@ run-call: denominated => 100000000
//@ run-call: negative => -10000000000
//@ run-call: remainder => 8
// ported-from: test/libsolidity/smtCheckerTests/types/rational_large_1.sol
// Number literals wider than 256 bits are exact in literal arithmetic.

contract C {
    function quotient() external pure returns (uint256) {
        return 1e79 / 1000;
    }

    function fraction() external pure returns (uint256) {
        return 1.5e78 / 1e71;
    }

    function hexLiteral() external pure returns (uint256) {
        return 0x10000000000000000000000000000000000000000000000000000000000000000 >> 4;
    }

    function denominated() external pure returns (uint256) {
        return 1e70 ether / 1e80;
    }

    function negative() external pure returns (int256) {
        return -1e100 / 1e90;
    }

    function remainder() external pure returns (uint256) {
        return 8e130 % 9;
    }
}
