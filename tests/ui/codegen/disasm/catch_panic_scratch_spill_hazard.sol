//@ compile-flags: -Ogas -Zdump=disasm-runtime
//@ filecheck:

// The scratch copy that reads a catch clause's selector has a length of `min(returndatasize, 32)`,
// which is at most 32 bytes at address 0, below the spill area. It is not a spill hazard, so
// the values `g` keeps live across the call are not reloaded and stored around the copy.
contract CatchPanicScratchHazard {
    function f() external pure { assert(false); }
    // CHECK-LABEL: ; === {{.*}}:CatchPanicScratchHazard (runtime) ===
    // CHECK: RETURNDATACOPY
    // CHECK: RETURNDATACOPY
    // CHECK-NOT: MSTORE
    // CHECK: PUSH1 0x23
    function g(uint256 a) external view returns (bytes32 r) {
        bytes32 h0 = keccak256(abi.encodePacked(a, uint256(0)));
        bytes32 h1 = keccak256(abi.encodePacked(a, uint256(1)));
        bytes32 h2 = keccak256(abi.encodePacked(a, uint256(2)));
        bytes32 h3 = keccak256(abi.encodePacked(a, uint256(3)));
        bytes32 h4 = keccak256(abi.encodePacked(a, uint256(4)));
        bytes32 h5 = keccak256(abi.encodePacked(a, uint256(5)));
        bytes32 h6 = keccak256(abi.encodePacked(a, uint256(6)));
        bytes32 h7 = keccak256(abi.encodePacked(a, uint256(7)));
        bytes32 h8 = keccak256(abi.encodePacked(a, uint256(8)));
        bytes32 h9 = keccak256(abi.encodePacked(a, uint256(9)));
        bytes32 h10 = keccak256(abi.encodePacked(a, uint256(10)));
        bytes32 h11 = keccak256(abi.encodePacked(a, uint256(11)));
        bytes32 h12 = keccak256(abi.encodePacked(a, uint256(12)));
        bytes32 h13 = keccak256(abi.encodePacked(a, uint256(13)));
        bytes32 h14 = keccak256(abi.encodePacked(a, uint256(14)));
        bytes32 h15 = keccak256(abi.encodePacked(a, uint256(15)));
        bytes32 h16 = keccak256(abi.encodePacked(a, uint256(16)));
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        try this.f() {} catch Panic(uint256) {}
        r = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17;
    }
}
