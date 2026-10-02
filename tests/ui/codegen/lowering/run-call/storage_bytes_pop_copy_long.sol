//@[mir] filecheck:
// CHECK: @module
//@[gas] compile-flags: -Zdump=evm-ir-runtime
//@[gas] filecheck: --check-prefix=SHORT
// SHORT-LABEL: @module StorageBytesPopCopyLong
// SHORT: push 31{{[[:space:]]+}}dup 2{{[[:space:]]+}}gt
// SHORT-NEXT: push bb{{[0-9]+}}
// SHORT-NEXT: jumpi
// SHORT-NEXT: push 31
// SHORT-NEXT: dup 2
// SHORT-NEXT: eq
// SHORT-NEXT: push bb{{[0-9]+}}
// SHORT-NEXT: jumpi
// SHORT-NEXT: swap 1
// SHORT-NEXT: push 2
// SHORT-NEXT: add
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
