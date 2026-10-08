//@ compile-flags: -O none --emit=bin --allow=5574
//@ normalize-stdout-test: "(?s).+" -> ""
//@ normalize-stderr-test: "size is [0-9]+ bytes" -> "size is <SIZE> bytes"
//~? ERROR: codegen cannot keep the generated EVM stack within 1024 words

import {DeepChain} from "./auxiliary/deep-call-chain.sol";

// Constructor code has its own call graph, which codegen checks against the
// 1024-word stack limit on its own. 1019 nested calls fit; the initcode size
// warning shows that this contract compiles.
contract ConstructorFits { //~ WARN: contract initcode size
    uint256 public value;

    constructor() {
        value = DeepChain.f1();
    }
}

// Codegen rejects one more nested call.
contract ConstructorOverflows {
    uint256 public value;

    constructor() {
        value = DeepChain.f0();
    }
}
