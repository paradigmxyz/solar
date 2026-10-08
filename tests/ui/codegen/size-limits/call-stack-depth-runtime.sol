//@ compile-flags: -O none --emit=bin --allow=5574
//@ normalize-stdout-test: "(?s).+" -> ""
//@ normalize-stderr-test: "size is [0-9]+ bytes" -> "size is <SIZE> bytes"
//~? ERROR: codegen cannot keep the generated EVM stack within 1024 words

import {DeepChain} from "./auxiliary/deep-call-chain.sol";

// Codegen adds up the stack words that every call path keeps alive and
// rejects runtime code whose deepest path cannot fit in 1024 words. 1017
// nested calls fit; the initcode size warning shows that this contract
// compiles.
contract RuntimeFits { //~ WARN: contract initcode size
    function run() external pure returns (uint256) {
        return DeepChain.f3();
    }
}

// Codegen rejects one more nested call.
contract RuntimeOverflows {
    function run() external pure returns (uint256) {
        return DeepChain.f2();
    }
}
