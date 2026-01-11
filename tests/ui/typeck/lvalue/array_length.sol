//@compile-flags: -Ztypeck

contract Test {
    uint256[] dynamicArray;
    uint256[10] fixedArray;
    uint256 state;

    function testDynamic() external {
        dynamicArray.length = state; //~ ERROR: member `length` is read-only and cannot be used to resize arrays
    }

    function testFixed() external {
        fixedArray.length = state; //~ ERROR: member `length` is read-only and cannot be used to resize arrays
    }

    function testParam(uint256[] memory arr) internal {
        arr.length = state; //~ ERROR: member `length` is read-only and cannot be used to resize arrays
    }
}
