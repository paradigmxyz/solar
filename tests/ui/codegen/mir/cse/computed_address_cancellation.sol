//@ codegen-matrix: standard
//@ run-call: cancel 0 => 42
//@ run-call: cancel 1 => 42
//@ run-call: cancel 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 42

contract ComputedAddressCancellation {
    function cancel(uint256 x) external pure returns (uint256 result) {
        assembly {
            let p := mload(0x40)
            let q := add(x, sub(p, x))
            mstore(q, 42)
            result := mload(p)
        }
    }
}
