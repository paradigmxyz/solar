//@ codegen-matrix: standard
//@ run-call: run() => 96

// solc leaves the free memory pointer where it was and returns 0; see CODEGEN-020 in
// docs/SOLC_DIVERGENCE.md.
contract C {
    function fail() external pure {
        assert(false);
    }

    function run() external view returns (uint256) {
        uint256 beforePtr;
        assembly { beforePtr := mload(0x40) }
        try this.fail() {
            return 999;
        } catch Panic(uint256) {
            uint256 afterPtr;
            assembly { afterPtr := mload(0x40) }
            return afterPtr - beforePtr;
        }
    }
}
