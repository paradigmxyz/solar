//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract CompoundAssign {
    // CHECK-LABEL: fn @value
    // CHECK: sload 0
    uint256 public value;

    // CHECK-LABEL: fn @add_to_value
    // CHECK: [[OLD:v[0-9]+]] = sload 0
    // CHECK: [[NEW:v[0-9]+]] = add [[OLD]], arg0
    // CHECK: sstore 0, [[NEW]]
    function add_to_value(uint256 x) public {
        value += x;
    }

    // CHECK-LABEL: fn @sub_from_value
    // CHECK: [[OLD:v[0-9]+]] = sload 0
    // CHECK: [[NEW:v[0-9]+]] = sub [[OLD]], arg0
    // CHECK: sstore 0, [[NEW]]
    function sub_from_value(uint256 x) public {
        value -= x;
    }

    // CHECK-LABEL: fn @mul_value
    // CHECK: [[OLD:v[0-9]+]] = sload 0
    // CHECK: [[NEW:v[0-9]+]] = mul [[OLD]], arg0
    // CHECK: sstore 0, [[NEW]]
    function mul_value(uint256 x) public {
        value *= x;
    }

    // CHECK-LABEL: fn @bump_post
    // CHECK: [[OLD:v[0-9]+]] = sload 0
    // CHECK: [[NEW:v[0-9]+]] = add [[OLD]], 1
    // CHECK: sstore 0, [[NEW]]
    // CHECK: mstore 128, [[OLD]]
    function bump_post() public returns (uint256) {
        return value++;
    }

    // CHECK-LABEL: fn @bump_pre
    // CHECK: [[OLD:v[0-9]+]] = sload 0
    // CHECK: [[NEW:v[0-9]+]] = add [[OLD]], 1
    // CHECK: sstore 0, [[NEW]]
    // CHECK: mstore 128, [[NEW]]
    function bump_pre() public returns (uint256) {
        return ++value;
    }
}
