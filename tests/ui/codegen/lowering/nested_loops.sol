//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract NestedLoops {
    // CHECK-LABEL: fn @sum_grid{{[( ]}}
    // CHECK: {{v[0-9]+}} = phi
    // CHECK: lt {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: {{v[0-9]+}} = mul {{v[0-9]+}}, {{v[0-9]+}}
    function sum_grid(uint256 n, uint256 m) public pure returns (uint256) {
        uint256 total = 0;
        for (uint256 i = 0; i < n; i++) {
            for (uint256 j = 0; j < m; j++) {
                total = total + i * j;
            }
        }
        return total;
    }

    // CHECK-LABEL: fn @find_first{{[( ]}}
    // CHECK: {{v[0-9]+}} = phi
    // CHECK: lt {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: add {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: eq {{v[0-9]+}}, {{v[0-9]+}}
    function find_first(uint256 n, uint256 target) public pure returns (uint256) {
        for (uint256 i = 0; i < n; i++) {
            for (uint256 j = 0; j < n; j++) {
                if (i + j == target) {
                    return i;
                }
            }
        }
        return n;
    }
}
