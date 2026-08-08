//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@ filecheck:

// Tuple assignment to EXISTING lvalues, `(a, b) = rhs`. `lower_assign` had no
// tuple case, so these silently assigned nothing (e.g. `(ok, ) = addr.call(d)`
// returned false regardless of the call). Handle a tuple RHS (including swaps,
// evaluating all right-hand values first), a low-level call (success flag plus
// returndata), and an ordinary multi-return call (first value plus the rest
// from memory). Runtime results verified equal to solc 0.8.30 separately.

contract C {
    // CHECK: push 0x1b8f5d50
    // CHECK: eq
    // CHECK: push 0x5030da75
    // CHECK: eq
    // CHECK: push 0xd96073cf
    // CHECK: eq
    // CHECK: calldatacopy
    // CHECK: {{^.*[ =]call[[:space:]]}}
    // CHECK: return
    function viaNamed(address t, bytes calldata d) external returns (bool ok) {
        (ok, ) = t.call(d);
    }

    // CHECK: push 36
    // CHECK: calldataload
    // CHECK: push 4
    // CHECK: calldataload
    // CHECK: return
    function swap(uint256 a, uint256 b) external pure returns (uint256, uint256) {
        (a, b) = (b, a);
        return (a, b);
    }

    function two() internal pure returns (uint256, uint256) {
        return (7, 9);
    }

    // CHECK: [[MULTI:bb[0-9]+]]:
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 4
    // CHECK-NEXT: gt
    // CHECK-NEXT: push [[GUARD_TARGET:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push [[RETURN_BLOCK:bb[0-9]+]]
    // CHECK-NEXT: push 7
    // CHECK: mstore
    // CHECK: push 9
    // CHECK: mstore
    // CHECK: jump
    // CHECK: [[RETURN_BLOCK]]:
    // CHECK: push 544
    // CHECK: mload
    // CHECK: jump
    function multi() external pure returns (uint256 x, uint256 y) {
        x = 100;
        y = 200;
        (x, y) = two();
    }
}
