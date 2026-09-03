// ABI validation of a static calldata array. solc validates each element
// lazily, when high-level code reads it, so a dirty element that is never read
// does not revert. solar validates the whole array as soon as any element is
// read, and reverts.
// Calldata `[0x101, 1]` (non-canonical `uint8` in element 0):
//   readSecond  solc returns 1, solar reverts
//   readFirst   both revert (the dirty element is read)
//   unused      both return 2 (nothing is read)
// Dynamic calldata arrays, calldata structs, and copies to memory agree.
contract CalldataStaticArrayValidation {
    function readSecond(uint8[2] calldata a) external pure returns (uint8) {
        return a[1];
    }

    function readFirst(uint8[2] calldata a) external pure returns (uint8) {
        return a[0];
    }

    function unused(uint8[2] calldata a) external pure returns (uint256) {
        return a.length;
    }

    function bools(bool[2] calldata a) external pure returns (bool) {
        return a[1];
    }

    function nested(uint8[2][2] calldata a) external pure returns (uint8) {
        return a[1][1];
    }

    // Still eager after ac68e1e40: passing the calldata array to an internal
    // function validates every element. solc returns 1 for [0x101, 1].
    function passToInternal(uint8[2] calldata a) external pure returns (uint8) {
        return second(a);
    }

    function second(uint8[2] calldata a) internal pure returns (uint8) {
        return a[1];
    }

    function copyToMemory(uint8[2] calldata a) external pure returns (uint8) {
        uint8[2] memory m = a;
        return m[1];
    }
}
