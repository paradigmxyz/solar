//@ codegen-matrix: standard
//@[mir] filecheck: --check-prefix=MIR
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: ConstantChainExpansion::head => 0x4c9d42f6ed5701ff2de66eb98531da3b11d09e3c3f7187274d0dea7e00c134a6
//@ run-call: ConstantChainExpansion::freshCopy => true
//@ run-call: ConstantChainHelpers::head => true
//@ run-call: ConstantChainHelpers::uncheckedUse => 0
//@ run-call-fail: ConstantChainHelpers::checkedUse() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// Each constant names the previous one twice, so inlining every use would lower `a0` 4096 times.
// Constant evaluation folds the whole chain.
contract ConstantChainExpansion {
    bytes32 constant a0 = keccak256("x");
    bytes32 constant a1 = keccak256(abi.encode(a0, a0));
    bytes32 constant a2 = keccak256(abi.encode(a1, a1));
    bytes32 constant a3 = keccak256(abi.encode(a2, a2));
    bytes32 constant a4 = keccak256(abi.encode(a3, a3));
    bytes32 constant a5 = keccak256(abi.encode(a4, a4));
    bytes32 constant a6 = keccak256(abi.encode(a5, a5));
    bytes32 constant a7 = keccak256(abi.encode(a6, a6));
    bytes32 constant a8 = keccak256(abi.encode(a7, a7));
    bytes32 constant a9 = keccak256(abi.encode(a8, a8));
    bytes32 constant a10 = keccak256(abi.encode(a9, a9));
    bytes32 constant a11 = keccak256(abi.encode(a10, a10));
    bytes32 constant a12 = keccak256(abi.encode(a11, a11));
    bytes constant ENCODED = abi.encode(a12, a12);

    // MIR-LABEL: fn @head()
    // MIR-NOT: icall
    // MIR: ret 0x4c9d42f6ed5701ff2de66eb98531da3b11d09e3c3f7187274d0dea7e00c134a6
    function head() external pure returns (bytes32) {
        return a12;
    }

    function freshCopy() external pure returns (bool) {
        bytes memory value = ENCODED;
        value[0] = 0xff;
        return keccak256(ENCODED) == keccak256(abi.encode(a12, a12));
    }
}

// `ecrecover` does not fold, so neither does the chain, and the uses of a large constant call a
// helper that lowers its initializer once.
contract ConstantChainHelpers {
    address constant s = ecrecover(bytes32(0), 27, bytes32(uint256(1)), bytes32(uint256(1)));
    bytes32 constant a0 = keccak256(abi.encode(s));
    bytes32 constant a1 = keccak256(abi.encode(a0, a0));
    bytes32 constant a2 = keccak256(abi.encode(a1, a1));
    bytes32 constant a3 = keccak256(abi.encode(a2, a2));
    bytes32 constant a4 = keccak256(abi.encode(a3, a3));
    bytes32 constant a5 = keccak256(abi.encode(a4, a4));
    bytes32 constant a6 = keccak256(abi.encode(a5, a5));
    bytes32 constant a7 = keccak256(abi.encode(a6, a6));

    // MIR-LABEL: fn @head()
    // MIR: icall @constant_value_[[A7:[0-9]+]]
    // MIR: fn @constant_value_[[A7]]()
    // MIR: icall @constant_value_[[A6:[0-9]+]]
    // MIR-NEXT: icall @constant_value_[[A6]]
    function head() external pure returns (bool) {
        return a7 == keccak256(abi.encode(a6, a6));
    }

    // The helper follows the arithmetic mode of the use, like an inlined initializer.
    uint8 constant MAX = 255;
    uint8 constant WRAPS = MAX + uint8(uint256(a6) & 0) + 1;

    function uncheckedUse() external pure returns (uint8) {
        unchecked {
            return WRAPS;
        }
    }

    function checkedUse() external pure returns (uint8) {
        return WRAPS;
    }
}
