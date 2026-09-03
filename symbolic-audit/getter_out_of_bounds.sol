// Public array getters called with an out-of-bounds index.
// solc (via-IR, 0.8.36) reverts with empty returndata.
// solar reverts with Panic(0x32).
contract GetterOutOfBounds {
    uint256[] public dynamicArray;
    uint256[2] public fixedArray;
}
