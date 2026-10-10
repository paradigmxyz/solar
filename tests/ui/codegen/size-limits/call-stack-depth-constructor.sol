//@ compile-flags: -O none --emit=bin --allow=5574
//@ normalize-stdout-test: "(?s).+" -> ""

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
