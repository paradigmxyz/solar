//@ revisions: semantic expanded
//@[semantic] compile-flags: -O none -Zdump=mir
//@[expanded] compile-flags: -O none -Zdump=mir -Zmir-pipeline=lower-builtins
//@[semantic] filecheck: --check-prefix=SEM
//@[expanded] filecheck:

contract AddmodMulmod {
    // SEM-LABEL: fn @am
    // SEM: icall checked_addmod<>, arg0, arg1, arg2
    // CHECK-LABEL: fn @am{{[( ]}}
    // CHECK: jumpi {{(arg[0-9]+|v[0-9]+)}},
    // CHECK: mstore 32, 18
    // CHECK: {{v[0-9]+}} = addmod arg0, arg1, arg2
    function am(uint x, uint y, uint n) public pure returns (uint) {
        return addmod(x, y, n);
    }

    // SEM-LABEL: fn @mm
    // SEM: icall checked_mulmod<>, arg0, arg1, arg2
    // CHECK-LABEL: fn @mm{{[( ]}}
    // CHECK: jumpi {{(arg[0-9]+|v[0-9]+)}},
    // CHECK: mstore 32, 18
    // CHECK: {{v[0-9]+}} = mulmod arg0, arg1, arg2
    function mm(uint x, uint y, uint n) public pure returns (uint) {
        return mulmod(x, y, n);
    }
}
