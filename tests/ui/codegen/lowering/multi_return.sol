//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract MultiReturn {
    // CHECK-LABEL: fn @div_mod{{[( ]}}
    // CHECK: {{v[0-9]+}} = div arg0, arg1
    // CHECK: {{v[0-9]+}} = mod arg0, arg1
    // CHECK: ret {{v[0-9]+}}, {{v[0-9]+}}
    function div_mod(uint256 a, uint256 b) public pure returns (uint256, uint256) {
        return (a / b, a % b);
    }

    // CHECK-LABEL: fn @min_max{{[( ]}}
    // CHECK: [[ORDERED:v[0-9]+]] = lt arg0, arg1
    // CHECK: jumpi [[ORDERED]],
    // CHECK-COUNT-2: ret
    function min_max(uint256 a, uint256 b) public pure returns (uint256, uint256) {
        if (a < b) {
            return (a, b);
        }
        return (b, a);
    }

    // CHECK-LABEL: fn @triple{{[( ]}}
    // CHECK: {{v[0-9]+}} = add arg0, arg0
    // CHECK: {{v[0-9]+}} = add {{v[0-9]+}}, arg0
    // CHECK: ret arg0, {{v[0-9]+}}, {{v[0-9]+}}
    function triple(uint256 x) public pure returns (uint256, uint256, uint256) {
        return (x, x + x, x + x + x);
    }
}
