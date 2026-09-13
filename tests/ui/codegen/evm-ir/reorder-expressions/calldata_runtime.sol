//@ codegen-matrix: standard
//@ run-call: words 7, 2 => 5, 8
//@ run-call: words 0, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 68
//@ run-call: words 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0

contract ClosedCalldata {
    function words(uint256, uint256) external pure returns (uint256 difference, uint256 quotient) {
        assembly {
            difference := sub(calldataload(4), calldataload(36))
            quotient := div(calldatasize(), add(calldataload(4), 1))
        }
    }
}
