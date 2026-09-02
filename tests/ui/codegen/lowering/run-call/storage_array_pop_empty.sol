//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call-fail: test => Panic(0x31)
// ported-from: test/libsolidity/semanticTests/array/pop/array_pop_empty_exception.sol

contract StorageArrayPopEmpty {
    uint256[] data;

    function test() external returns (bool) {
        data.pop();
        return true;
    }
}
