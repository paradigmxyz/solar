//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: test => 2, 1, 1
// ported-from: test/libsolidity/semanticTests/array/pop/byte_array_pop.sol

contract StorageBytesPop {
    bytes data;

    function test() external returns (uint256 x, uint256 y, uint256 l) {
        data.push(0x07);
        data.push(0x03);
        x = data.length;
        data.pop();
        data.pop();
        data.push(0x02);
        y = data.length;
        l = data.length;
    }
}
