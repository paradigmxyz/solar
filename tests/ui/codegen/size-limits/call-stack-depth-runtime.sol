//@ compile-flags: -O none --emit=bin --allow=5574
//@ normalize-stdout-test: "(?s).+" -> ""

import {DeepChain} from "./auxiliary/deep-call-chain.sol";

// Codegen adds up the stack words that every call path keeps alive. Each
// caller in this chain keeps only its return address below the callee, so
// 1017 and 1018 nested calls both fit in 1024 words. A chain that does not
// fit moves its callers' words to memory, as in
// `lowering/run-call/deep_call_chain_stack_limit.sol`.
contract RuntimeFits {
    function run() external pure returns (uint256) {
        return DeepChain.f3();
    }
}

contract RuntimeDeep {
    function run() external pure returns (uint256) {
        return DeepChain.f2();
    }
}
