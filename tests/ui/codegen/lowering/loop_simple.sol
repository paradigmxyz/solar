//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract LoopSimple {
    // CHECK-LABEL: fn @sum_to{{[( ]}}
    // CHECK: {{v[0-9]+}} = phi
    // CHECK: lt {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: ret {{v[0-9]+}}
    // CHECK: {{v[0-9]+}} = add {{v[0-9]+}}, {{v[0-9]+}}
    function sum_to(uint256 n) public pure returns (uint256) {
        uint256 total = 0;
        for (uint256 i = 0; i < n; i++) {
            total = total + i;
        }
        return total;
    }
}
