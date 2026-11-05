//@ compile-flags: -O none --emit=bin --allow=5574
//@ filecheck:
// CHECK-DAG: :ConstructorFits":{"bin":"{{[0-9a-f]+}}"}
// CHECK-DAG: :ConstructorDeep":{"bin":"{{[0-9a-f]+}}"}


import {DeepChain} from "./auxiliary/deep-call-chain.sol";

// Constructor code has its own call graph, which codegen checks against the
// 1024-word stack limit on its own. Each caller in this chain keeps only its
// return address below the callee, so 1019 and 1020 nested calls both fit.
contract ConstructorFits {
    uint256 public value;

    constructor() {
        value = DeepChain.f1();
    }
}

contract ConstructorDeep {
    uint256 public value;

    constructor() {
        value = DeepChain.f0();
    }
}
