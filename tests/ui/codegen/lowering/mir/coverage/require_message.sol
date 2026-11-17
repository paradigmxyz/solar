//@compile-flags: -O none -Zinstrument-coverage -Zdump=mir
//@filecheck: --enable-var-scope

// A `require` counts its condition when it holds before its message runs when the message may
// revert while it is evaluated, which is no failure of the condition. A message that cannot, a
// constant, a variable, or a custom error with such arguments, leaves the counter after the check,
// so the code after the `require` stays in one block.
error E(uint256);

string constant MESSAGE = "no";

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

    // CHECK-LABEL: fn @named(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: icall require<short_string>, arg0
    // CHECK-NEXT: coverage {{[0-9]+}}
    function named(bool ok) external pure {
        require(ok, MESSAGE);
    }

    // CHECK-LABEL: fn @custom(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: coverage {{[0-9]+}}
    // CHECK-NEXT: icall require<custom_error [word]>, arg0
    // CHECK-NEXT: coverage {{[0-9]+}}
    function custom(bool ok, uint256 x) external pure {
        require(ok, E(x));
    }
}
