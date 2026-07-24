//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract C {
    // CHECK-LABEL: fn @start{{[( ]}}
    // CHECK: loadimmutable 0
    uint256 public immutable start;

    // CHECK-LABEL: fn @duration{{[( ]}}
    // CHECK: loadimmutable 32
    uint256 public immutable duration;

    // CHECK-LABEL: fn @_anonymous{{[( ]}}
    // CHECK: mstore 0x2000, arg0
    // CHECK: [[START:v[0-9]+]] = mload 0x2000
    // CHECK: [[DURATION:v[0-9]+]] = add [[START]], 1
    // CHECK: mstore 0x2020, [[DURATION]]
    constructor(uint256 s) {
        start = s;
        // Constructor-context reads use the staged scratch word: the runtime
        // placeholders are only patched in the returned copy of the code.
        duration = start + 1;
    }

    // CHECK-LABEL: fn @end{{[( ]}}
    // CHECK: [[START:v[0-9]+]] = loadimmutable 0
    // CHECK: [[DURATION:v[0-9]+]] = loadimmutable 32
    // CHECK: add [[START]], [[DURATION]]
    function end() public view returns (uint256) {
        return start + duration;
    }
}
