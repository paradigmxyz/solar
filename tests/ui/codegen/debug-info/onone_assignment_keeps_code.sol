//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ run-call: f 1 => 7
//@ run-call: f 2 => 8
//@ run-call: copies 5, 6, true => 5, 6, true
//@ run-call: copies 7, 255, false => 7, 255, false
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// At `-O none` the assignment `y = 8` keeps a push of its own at its own location, instead of
// leaving the constant to the push at the function's exit, so a coverage run counts the statement.
contract C {
    // CHECK-LABEL: ; function f(
    // The assignment `y = 8` is five bytes long.
    // CHECK: push 8 !metadata(span=[[#START:]]..
    // CHECK-SAME: [[#START + 5]])
    // CHECK: return !metadata(span={{[0-9]+}}..{{[0-9]+}}, exit=return)
    function f(uint256 x) external pure returns (uint256 y) {
        if (x == 1) {
            return 7;
        }
        y = 8;
    }

    function copies(uint256 x, uint8 a, bool b) external pure returns (uint256 r, uint8 s, bool t) {
        r = x;
        s = a;
        t = b;
    }
}
