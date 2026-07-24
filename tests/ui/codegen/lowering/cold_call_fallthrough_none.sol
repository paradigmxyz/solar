//@compile-flags: -Zcodegen -O none -Zdump=evm-ir-runtime
//@filecheck: --enable-var-scope

contract ColdCallFallthroughNone {
    // CHECK-LABEL: @module runtime
    // CHECK: push 128
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 0{{$}}
    // CHECK-NEXT: push 4{{$}}
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: eq
    // CHECK-NEXT: iszero
    // CHECK-NEXT: push [[HOT:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: jump [[COLD:bb[0-9]+]]
    // CHECK: [[COLD]]:
    // CHECK: jump
    // CHECK: [[HOT]]:
    // CHECK: return
    function nonzero(uint256 value) external pure returns (uint256) {
        if (value == 0) abort(value);
        return value;
    }

    function abort(uint256 value) internal pure {
        if (value == 0) revert("zero");
        if (value == 1) revert("one");
        if (value == 2) revert("two");
        revert("other");
    }
}
