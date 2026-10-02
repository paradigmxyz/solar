//@compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

contract Test {
    // The decoded `account` stays resident through the comparison chain: each
    // test duplicates it instead of reloading calldata, and the last one
    // consumes it.
    // CHECK: push 0xc21f7bbb
    // CHECK: eq
    // CHECK: push 1{{$}}
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: eq
    // CHECK-NEXT: push [[ONE:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 2{{$}}
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: eq
    // CHECK-NEXT: push [[TWO:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 3{{$}}
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: eq
    // CHECK-NEXT: push [[THREE:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 4{{$}}
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: eq
    // CHECK-NEXT: push [[FOUR:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 5{{$}}
    // CHECK-NEXT: eq
    // CHECK-NEXT: push [[FIVE:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: [[FIVE]]:
    // CHECK-NEXT: push 5
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: add
    // CHECK: [[FOUR]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: add
    // CHECK: [[THREE]]:
    // CHECK-NEXT: push 3
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: add
    // CHECK: [[TWO]]:
    // CHECK-NEXT: push 2
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: add
    // CHECK: [[ONE]]:
    // CHECK: push 1
    // CHECK-NEXT: add
    function select(address account, uint256 value) external pure returns (uint256) {
        if (account == address(1)) return value + 1;
        if (account == address(2)) return value + 2;
        if (account == address(3)) return value + 3;
        if (account == address(4)) return value + 4;
        if (account == address(5)) return value + 5;
        return value;
    }
}
