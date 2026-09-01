//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: DeleteMemoryStructReference::f => 0, 7
//@ run-call: DeleteMemoryStructReference::fixedArray => 0, 7

contract DeleteMemoryStructReference {
    struct S {
        uint256[] values;
    }

    function f() external pure returns (uint256, uint256) {
        S memory a;
        a.values = new uint256[](1);
        a.values[0] = 7;
        S memory b = a;
        delete a;
        return (a.values.length, b.values[0]);
    }

    function fixedArray() external pure returns (uint256, uint256) {
        uint256[2] memory a;
        a[0] = 7;
        uint256[2] memory b = a;
        delete a;
        return (a[0], b[0]);
    }
}
