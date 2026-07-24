//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract AddmodMulmod {
    // CHECK-LABEL: fn @am
    // CHECK: {{v[0-9]+}} = addmod arg0, arg1, arg2
    function am(uint x, uint y, uint n) public pure returns (uint) {
        return addmod(x, y, n);
    }

    // CHECK-LABEL: fn @mm
    // CHECK: {{v[0-9]+}} = mulmod arg0, arg1, arg2
    function mm(uint x, uint y, uint n) public pure returns (uint) {
        return mulmod(x, y, n);
    }
}
