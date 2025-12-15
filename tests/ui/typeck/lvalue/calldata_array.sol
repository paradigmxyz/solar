//@compile-flags: -Ztypeck

contract Test {
    uint256 state;
    uint256 idx;

    function test(uint256[] calldata arr) external {
        arr[idx] = state; //~ ERROR: calldata arrays are read-only
    }

    function testBytes(bytes calldata data) external {
        bytes1 b;
        data[idx] = b; //~ ERROR: calldata arrays are read-only
    }

    function testNested(uint256[][] calldata nested) external {
        nested[idx][idx] = state; //~ ERROR: calldata arrays are read-only
    }
}
