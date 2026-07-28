//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract C {
    // CHECK-LABEL: fn @start{{[( ]}}
    // CHECK: loadimmutable start
    uint256 public immutable start;

    // CHECK-LABEL: fn @duration{{[( ]}}
    // CHECK: loadimmutable duration
    uint256 public immutable duration;

    // CHECK-LABEL: fn @_anonymous{{[( ]}}
    // CHECK: storeimmutable start, arg0
    // CHECK: [[START:v[0-9]+]] = loadimmutable start
    // CHECK: [[DURATION:v[0-9]+]] = add [[START]], 1
    // CHECK: storeimmutable duration, [[DURATION]]
    constructor(uint256 s) {
        start = s;
        // Constructor-context reads observe the current assigned value: runtime
        // placeholders are only patched in the returned copy of the code.
        duration = start + 1;
    }

    // CHECK-LABEL: fn @end{{[( ]}}
    // CHECK: [[START:v[0-9]+]] = loadimmutable start
    // CHECK: [[DURATION:v[0-9]+]] = loadimmutable duration
    // CHECK: add [[START]], [[DURATION]]
    function end() public view returns (uint256) {
        return start + duration;
    }
}
