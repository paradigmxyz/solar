//@ compile-flags: -O gas
//@ run-call: f => 1

// A chain of self-doublings is a bounded offset only while the bound stays small, and the walk
// that decides it looks at each word once, not once per path to it. Sixty doublings of the
// calldata size took the walk past any reasonable time.

contract C {
    function f() external pure returns (uint256) {
        assembly {
            let p := mload(0x40)
            let n := calldatasize()
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            n := add(n, n)
            if gt(calldatasize(), 1000) { mstore(add(p, n), 1) }
        }
        return 1;
    }
}
