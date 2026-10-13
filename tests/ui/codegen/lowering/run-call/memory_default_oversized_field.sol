//@ codegen-matrix: standard
//@ run-call-fail: dynamicElement() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
// A struct array element counts one word per field, so its static array fields
// can be too large to allocate. Their default must not be a null pointer, which
// reads reserved memory such as the free memory pointer; allocating them reverts
// with `Panic(0x41)`, as solc's allocator does past the memory limit.
contract C {
    struct T {
        uint256[2 ** 60] a;
    }

    function dynamicElement() public pure returns (uint256) {
        T[] memory t = new T[](1);
        return t[0].a[2];
    }
}
