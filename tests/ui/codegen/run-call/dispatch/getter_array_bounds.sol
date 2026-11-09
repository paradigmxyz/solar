//@ codegen-matrix: standard
//@ run-call-fail: dynamicArray 0 => 0x
//@ run-call-fail: fixedArray 2 => 0x

contract GetterArrayBounds {
    uint256[] public dynamicArray;
    uint256[2] public fixedArray;
}
