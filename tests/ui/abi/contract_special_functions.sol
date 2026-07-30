//@ compile-flags: --emit=abi,hashes --pretty-json
//@ filecheck:

// CHECK-LABEL: "ROOT/tests/ui/abi/contract_special_functions.sol:A":
// CHECK-NOT: "type": "constructor"
// CHECK: "type": "fallback"
// CHECK-LABEL: "ROOT/tests/ui/abi/contract_special_functions.sol:B":
// CHECK-NOT: "type": "constructor"
// CHECK: "type": "fallback"
// CHECK: "type": "receive"
// CHECK-LABEL: "ROOT/tests/ui/abi/contract_special_functions.sol:C":
// CHECK: "type": "constructor"
// CHECK: "type": "fallback"
// CHECK-LABEL: "ROOT/tests/ui/abi/contract_special_functions.sol:D":
// CHECK: "type": "constructor"
// CHECK: "stateMutability": "payable"
// CHECK: "type": "fallback"
// CHECK: "type": "receive"
// CHECK-LABEL: "ROOT/tests/ui/abi/contract_special_functions.sol:E":
// CHECK-NOT: "type": "constructor"
// CHECK: "type": "fallback"
// CHECK-LABEL: "ROOT/tests/ui/abi/contract_special_functions.sol:F":
// CHECK-NOT: "type": "constructor"
// CHECK: "type": "fallback"
// CHECK: "type": "receive"

// Abstract contracts don't emit constructors.
abstract contract A {
    constructor() payable {}
    fallback() external {}
}

abstract contract B is A {
    receive() external payable {}
}

contract C {
    constructor() {}
    fallback() external {}
}

contract D is C {
    constructor() payable {}
    receive() external payable {}
}

// Inherits `C.fallback`, but not the constructor.
contract E is C {}

// Inherits `C.fallback` and `D.receive`, but not any of the constructors.
contract F is D {}
