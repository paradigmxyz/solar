//@compile-flags: -Ztypeck

contract Test {
    bytes32 fixedBytes;
    bytes1 singleByte;
    bytes1 source;
    uint256 idx;

    function test() external {
        fixedBytes[idx] = source; //~ ERROR: single bytes in fixed bytes arrays cannot be modified
        singleByte[idx] = source; //~ ERROR: single bytes in fixed bytes arrays cannot be modified
    }

    function testLocal() external {
        bytes32 local;
        local[idx] = source; //~ ERROR: single bytes in fixed bytes arrays cannot be modified
    }

    function testParam(bytes32 param) external {
        param[idx] = source; //~ ERROR: single bytes in fixed bytes arrays cannot be modified
    }
}
