//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract Storage {
    // CHECK-LABEL: fn @count
    // CHECK: sload 0
    uint256 public count;

    // CHECK-LABEL: fn @increment
    // CHECK: [[OLD:v[0-9]+]] = sload 0
    // CHECK: [[NEW:v[0-9]+]] = add [[OLD]], 1
    // CHECK: sstore 0, [[NEW]]
    function increment() public {
        count = count + 1;
    }

    // CHECK-LABEL: fn @set
    // CHECK: sstore 0, arg0
    function set(uint256 v) public {
        count = v;
    }

    // CHECK-LABEL: fn @get
    // CHECK: sload 0
    function get() public view returns (uint256) {
        return count;
    }
}
