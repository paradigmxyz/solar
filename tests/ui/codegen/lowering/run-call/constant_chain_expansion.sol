//@ codegen-matrix: standard
//@[mir] filecheck: --check-prefix=MIR
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: ConstantChainExpansion::head => 0x4c9d42f6ed5701ff2de66eb98531da3b11d09e3c3f7187274d0dea7e00c134a6
//@ run-call: ConstantChainExpansion::freshCopy => true
//@ run-call: ConstantChainMembers::last => 0

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

// Constants named by member access fold too.
contract ConstantChainMembers {
    // A typed shift drops the bits past its type.
    uint8 constant X0 = uint8(1) << 8;
    uint8 constant X1 = ConstantChainMembers.X0 | ConstantChainMembers.X0;
    uint8 constant X2 = ConstantChainMembers.X1 | ConstantChainMembers.X1;
    uint8 constant X3 = ConstantChainMembers.X2 | ConstantChainMembers.X2;
    uint8 constant X4 = ConstantChainMembers.X3 | ConstantChainMembers.X3;
    uint8 constant X5 = ConstantChainMembers.X4 | ConstantChainMembers.X4;
    uint8 constant X6 = ConstantChainMembers.X5 | ConstantChainMembers.X5;
    uint8 constant X7 = ConstantChainMembers.X6 | ConstantChainMembers.X6;
    uint8 constant X8 = ConstantChainMembers.X7 | ConstantChainMembers.X7;
    uint8 constant X9 = ConstantChainMembers.X8 | ConstantChainMembers.X8;
    uint8 constant X10 = ConstantChainMembers.X9 | ConstantChainMembers.X9;

    // MIR-LABEL: fn @last()
    // MIR-NOT: icall
    // MIR: ret i8 0
    function last() external pure returns (uint8) {
        return X10;
    }
}
