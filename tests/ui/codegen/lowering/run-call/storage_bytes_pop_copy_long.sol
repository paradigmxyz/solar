//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: test => 0x0303030303030303030303030303030303030303030303030303030303
//@ run-call: testNoPop => 0x030303030303030303030303030303030303030303030303030303030303030303
//@ run-call: testOnePop => 3
// ported-from: test/libsolidity/semanticTests/array/pop/byte_array_pop_copy_long.sol

contract StorageBytesPopCopyLong {
    bytes data;

    function test() external returns (bytes memory) {
        for (uint256 i; i < 33; ++i) data.push(0x03);
        for (uint256 j; j < 4; ++j) data.pop();
        return data;
    }

    function testNoPop() external returns (bytes memory) {
        for (uint256 i; i < 33; ++i) data.push(0x03);
        return data;
    }

    function testOnePop() external returns (uint8) {
        for (uint256 i; i < 33; ++i) data.push(0x03);
        data.pop();
        return uint8(data[0]);
    }
}
