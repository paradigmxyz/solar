//@ codegen-matrix: standard
//@ run-call: overwrite 0 => 9
//@ run-call: overwrite 1 => 9
//@ run-call: overwrite 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 9

contract AllocationWriteBounds {
    function overwrite(uint256 x) external pure returns (uint256) {
        uint256[1] memory a;
        uint256[1] memory b;
        b[0] = 7;
        assembly {
            mstore(add(x, sub(add(a, 32), x)), 9)
        }
        return b[0];
    }
}
