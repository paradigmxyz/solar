//@ codegen-matrix: standard
//@ run-call: write => 7, 42
// A storage array too large for memory is still passed by its base slot, not by a loaded word.
contract C {
    uint256 marker = 7;
    uint256[1 << 64] data;

    function set(uint256[1 << 64] storage target) internal {
        target[0] = 42;
    }

    function write() public returns (uint256, uint256) {
        set(data);
        return (marker, data[0]);
    }
}
