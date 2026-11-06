//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: access => 1, 0, 0
// ported-from: test/libsolidity/semanticTests/array/pop/array_pop_isolated.sol
// ported-from: test/libsolidity/semanticTests/array/pop/byte_array_pop_isolated.sol

contract IsolatedArrayPop {
    uint256[][] private values;
    bytes private data;

    function access()
        external
        returns (uint256 index, uint256 arrayLength, uint256 bytesLength)
    {
        values.push();
        values.push();
        values[index++].pop;
        data.pop;
        arrayLength = values[0].length;
        bytesLength = data.length;
    }
}
