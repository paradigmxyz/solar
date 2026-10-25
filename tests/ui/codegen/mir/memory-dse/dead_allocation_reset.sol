//@ codegen-matrix: standard
//@ run-call: f 7 => 7
//@ run-call: neighbour 7 => 7

// Rewinding the free-memory pointer puts the next object over an earlier one, and assembly can read
// an object past its end into its neighbour, so neither object's stores are dead.
contract DeadAllocationReset {
    function f(uint256 x) external pure returns (uint256 r) {
        uint256[] memory o = new uint256[](2);
        assembly {
            mstore(0x40, o)
        }
        uint256[] memory a = new uint256[](2);
        a[0] = x;
        r = o[0];
    }

    function neighbour(uint256 x) external pure returns (uint256 r) {
        uint256[1] memory a;
        uint256[1] memory b;
        b[0] = x;
        assembly ("memory-safe") {
            r := mload(add(a, 0x20))
        }
    }
}
