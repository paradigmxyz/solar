//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime --pretty-json
//@ filecheck:

contract StackPhiLoop {
    // CHECK: push 0x50d1f082
    // CHECK: eq
    // CHECK-NEXT: push [[NESTED:bb[0-9]+]]
    // CHECK: push 0x71b76bb2
    // CHECK: eq
    // CHECK-NEXT: push [[CARRIED:bb[0-9]+]]
    // CHECK: push 0xfb08deb2
    // CHECK: eq
    // CHECK-NEXT: push [[SEQUENTIAL:bb[0-9]+]]
    // CHECK: [[CARRIED]]:
    // CHECK: jump [[CARRIED_HEADER:bb[0-9]+]]
    // CHECK: [[CARRIED_HEADER]]:
    // CHECK: push [[CARRIED_BODY:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: return
    // CHECK: jump [[CARRIED_HEADER]]
    // CHECK: [[CARRIED_BODY]]:
    // CHECK: push [[CARRIED_LATCH:bb[0-9]+]]
    // CHECK: jumpi
    function loopCarried(uint256 n, bool flag) public pure returns (uint256) {
        uint256 step = flag ? 7 : 11;
        uint256 acc = 0;
        for (uint256 i = 0; i < n; i++) {
            acc += i * 3 + step;
        }
        return acc;
    }

    // CHECK: [[SEQUENTIAL]]:
    // CHECK: jump [[FIRST_HEADER:bb[0-9]+]]
    // CHECK: [[FIRST_HEADER]]:
    // CHECK: push [[FIRST_EXIT:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: push 1
    // CHECK: push [[FIRST_HEADER]]
    // CHECK-NEXT: jumpi
    // CHECK: [[FIRST_EXIT]]:
    // CHECK: jump [[SECOND_HEADER:bb[0-9]+]]
    // CHECK: [[SECOND_HEADER]]:
    // CHECK: push [[SECOND_BODY:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: jump [[SECOND_HEADER]]
    function sequential(uint256 a, uint256 b) public pure returns (uint256) {
        uint256 acc = 0;
        for (uint256 i = 0; i < a; i++) {
            acc += i + 1;
        }
        for (uint256 j = 0; j < b; j++) {
            acc += j * 2 + 3;
        }
        return acc;
    }

    // CHECK: [[NESTED]]:
    // CHECK: jump [[OUTER_HEADER:bb[0-9]+]]
    // CHECK: [[OUTER_HEADER]]:
    // CHECK: push [[OUTER_BODY:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: [[OUTER_BODY]]:
    // CHECK: jump [[INNER_HEADER:bb[0-9]+]]
    // CHECK: [[INNER_HEADER]]:
    // CHECK: push [[INNER_BODY:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: jump [[OUTER_HEADER]]
    // CHECK: jump [[INNER_HEADER]]
    function nested(uint256 outer, uint256 inner) public pure returns (uint256) {
        uint256 acc = 0;
        for (uint256 i = 0; i < outer; i++) {
            for (uint256 j = 0; j < inner; j++) {
                acc += i + j + 1;
            }
        }
        return acc;
    }
}
