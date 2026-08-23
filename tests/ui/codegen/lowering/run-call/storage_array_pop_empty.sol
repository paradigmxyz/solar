//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call-fail: test() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000031
// ported-from: test/libsolidity/semanticTests/array/pop/array_pop_empty_exception.sol

contract StorageArrayPopEmpty {
    uint256[] data;

    function test() external returns (bool) {
        data.pop();
        return true;
    }
}
