//@ compile-flags: -O none --emit=bin --allow=5574
//@ normalize-stdout-test: "(?s).+" -> ""

import {DeepChain} from "./auxiliary/deep-call-chain.sol";

// Each caller in this chain keeps only its return address below the callee, so
// 1017 and 1018 nested calls both fit in 1024 words. Only the EVM IR verifier
// bounds the stack, and only for code that provably passes the limit, as in
// `lowering/evm-ir/stack-too-deep/deep_call_chain_stack_limit.sol`.
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
