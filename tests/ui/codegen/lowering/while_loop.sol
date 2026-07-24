//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract WhileLoop {
    // CHECK-LABEL: fn @count_down
    // CHECK: [[I:v[0-9]+]] = mload 160
    // CHECK: gt [[I]], 0
    // CHECK: returndata
    // CHECK: [[BODY_I:v[0-9]+]] = mload 160
    // CHECK: sub [[BODY_I]], 1
    function count_down(uint256 n) public pure returns (uint256) {
        uint256 i = n;
        while (i > 0) {
            i = i - 1;
        }
        return i;
    }

    // CHECK-LABEL: fn @do_at_least_once
    // CHECK: [[I:v[0-9]+]] = mload 160
    // CHECK: {{v[0-9]+}} = add [[I]], 1
    // CHECK: returndata
    // CHECK: [[LOOP_I:v[0-9]+]] = mload 160
    // CHECK: lt [[LOOP_I]], arg0
    function do_at_least_once(uint256 n) public pure returns (uint256) {
        uint256 i = 0;
        do {
            i = i + 1;
        } while (i < n);
        return i;
    }

    // CHECK-LABEL: fn @break_when_found
    // CHECK: [[I:v[0-9]+]] = mload 160
    // CHECK: lt [[I]], arg0
    // CHECK: returndata
    // CHECK: [[BODY_I:v[0-9]+]] = mload 160
    // CHECK: eq [[BODY_I]], arg1
    // CHECK: add {{v[0-9]+}}, 1
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
