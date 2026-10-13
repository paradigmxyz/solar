//@ compile-flags: -Ogas -Zdump=evm-ir
//@ filecheck:
//@ run-call: Twin::value => 7
//@ run-call: WithArgs::value; constructor=[40] => 40

contract Child {
    function value() external pure returns (uint256) {
        return 7;
    }
}

// The runtime code is the creation code's last data entry. Packing can store it inside
// equal embedded bytes.
// CHECK-LABEL: @module Twin_deployment
// CHECK: Child_creation_code_0: hex"
// CHECK-NOT: Twin_runtime_code
// CHECK: push_data Child_creation_code_0+17
// CHECK-NEXT: push 0
// CHECK-NEXT: codecopy
contract Twin {
    constructor() {
        new Child();
    }

    function value() external pure returns (uint256) {
        return 7;
    }
}

// Constructor arguments follow the runtime code, so `CODESIZE` keeps the data layout.
// CHECK-LABEL: @module WithArgs_deployment
// CHECK: Child_creation_code_0: hex"
// CHECK-NEXT: WithArgs_runtime_code_1: hex"
// CHECK: push_data WithArgs_runtime_code_1+{{[0-9]+}}
// CHECK-NEXT: codesize
// CHECK: push_data Child_creation_code_0
// CHECK: push_data WithArgs_runtime_code_1{{$}}
contract WithArgs {
    uint256 immutable stored;

    constructor(uint256 v) {
        new Child();
        stored = v;
    }

    function value() external view returns (uint256) {
        return stored;
    }
}
