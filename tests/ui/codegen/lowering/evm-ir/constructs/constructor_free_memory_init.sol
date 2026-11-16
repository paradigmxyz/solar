//@ compile-flags: -Ogas -Zdump=evm-ir
//@ filecheck:
//@ run-call: Plain::value => 7

// A constructor that never reads the free memory pointer leaves it unset, as a
// runtime entry does; one that allocates, or that takes arguments to place
// above static memory, still starts the heap.
// CHECK-LABEL: Plain (creation)
// CHECK-NOT: push 64
// CHECK: sstore
// CHECK-LABEL: Allocating (creation)
// CHECK: push 64
// CHECK-NEXT: mstore
// CHECK-LABEL: WithArgument (creation)
// CHECK: codecopy
// CHECK: push 64
// CHECK-NEXT: mstore
contract Plain {
    uint256 public value;

    constructor() {
        value = 7;
    }
}

contract Allocating {
    bytes32 public digest;

    constructor() {
        uint256[] memory words = new uint256[](3);
        words[2] = block.number;
        digest = keccak256(abi.encode(words));
    }
}

contract WithArgument {
    uint256 public value;

    constructor(uint256 v) {
        value = v;
    }
}
