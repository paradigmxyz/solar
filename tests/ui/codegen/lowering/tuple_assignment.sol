//@compile-flags: -Zdump=evm-ir-runtime
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
    // CHECK-NEXT: push [[MULTI:bb[0-9]+]]
    // CHECK: push 0x5030da75
    // CHECK: eq
    // CHECK-NEXT: push [[NAMED:bb[0-9]+]]
    // CHECK: push 0xd96073cf
    // CHECK: eq
    // CHECK-NEXT: push [[SWAP:bb[0-9]+]]
    // CHECK: [[NAMED]]:
    // CHECK: calldatacopy
    // CHECK: {{^.*[ =]call[[:space:]]}}
    // CHECK: return
    function viaNamed(address t, bytes calldata d) external returns (bool ok) {
        (ok, ) = t.call(d);
    }

    // CHECK: [[SWAP]]:
    // CHECK: push 36
    // CHECK-NEXT: calldataload
    // CHECK: push 4
    // CHECK-NEXT: calldataload
    // CHECK: jump [[PAIR_RETURN:bb[0-9]+]]
    // CHECK: [[PAIR_RETURN]]:
    // CHECK: return
    function swap(uint256 a, uint256 b) external pure returns (uint256, uint256) {
        (a, b) = (b, a);
        return (a, b);
    }

    function two() internal pure returns (uint256, uint256) {
        return (7, 9);
    }

    // CHECK: [[MULTI]]:
    // The tiny-leaf inliner exposes `two()` as constants and removes its call frame.
    // The entry's free-memory initialization and the following static allocation share their
    // identical base push.
    // CHECK: push 224
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK: push 9
    // CHECK-NEXT: swap 1
    // CHECK: mstore
    // CHECK: push 7
    // CHECK-NEXT: push 128
    // CHECK-NEXT: mstore
    // CHECK: jump [[PAIR_RETURN]]
    function multi() external pure returns (uint256 x, uint256 y) {
        x = 100;
        y = 200;
        (x, y) = two();
    }
}
