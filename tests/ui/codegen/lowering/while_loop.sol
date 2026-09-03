//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract WhileLoop {
    // CHECK-LABEL: fn @count_down{{[( ]}}
    // CHECK: {{v[0-9]+}} = phi
    // CHECK: gt {{v[0-9]+}}, 0
    // CHECK: ret {{v[0-9]+}}
    // CHECK: {{v[0-9]+}} = sub {{v[0-9]+}}, 1
    function count_down(uint256 n) public pure returns (uint256) {
        uint256 i = n;
        while (i > 0) {
            i = i - 1;
        }
        return i;
    }

    // CHECK-LABEL: fn @do_at_least_once{{[( ]}}
    // CHECK: {{v[0-9]+}} = phi
    // CHECK: {{v[0-9]+}} = add {{v[0-9]+}}, 1
    // CHECK: lt {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: ret {{v[0-9]+}}
    function do_at_least_once(uint256 n) public pure returns (uint256) {
        uint256 i = 0;
        do {
            i = i + 1;
        } while (i < n);
        return i;
    }

    // CHECK-LABEL: fn @break_when_found{{[( ]}}
    // CHECK: {{v[0-9]+}} = phi
    // CHECK: lt {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: eq {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: {{v[0-9]+}} = add {{v[0-9]+}}, 1
    function break_when_found(uint256 n, uint256 target) public pure returns (uint256) {
        uint256 i = 0;
        while (i < n) {
            if (i == target) {
                break;
            }
            i = i + 1;
        }
        return i;
    }
}
