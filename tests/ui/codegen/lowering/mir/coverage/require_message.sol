//@compile-flags: -O none -Zinstrument-coverage -Zdump=mir
//@filecheck: --enable-var-scope

// A `require` counts its condition when it holds before its message runs, so a message that
// reverts while it is evaluated is no failure of the condition.
contract RequireMessage {
    // CHECK-LABEL: fn @f(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 1
    // CHECK-NEXT: coverage 2
    // CHECK-NEXT: jumpi arg1, [[HOLDS:bb[0-9]+]],
    // CHECK: [[HOLDS]]:
    // CHECK-NEXT: coverage 3
    // The message and its bounds check come after.
    // CHECK: icall panic_if<0x32>
    // CHECK: icall require<error_string>
    function f(bytes memory a, bool ok) external pure {
        require(ok, string(abi.encodePacked(a[5])));
    }
}
