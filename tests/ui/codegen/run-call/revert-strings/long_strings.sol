//@ revisions: default strip debug
//@[strip] compile-flags: --revert-strings strip
//@[debug] compile-flags: --revert-strings debug

//@ run-call: requireLong 1 =>
//@[default,debug] run-call-fail: requireLong 0 => Error("a require message that is longer than thirty-two bytes")
//@[strip] run-call-fail: requireLong 0 => 0x

//@[default,debug] run-call-fail: revertLong => Error("a revert message that is longer than thirty-two bytes")
//@[strip] run-call-fail: revertLong => 0x

//@ run-call: requireConstant 1 =>
//@[default,debug] run-call-fail: requireConstant 0 => Error("a constant message that is longer than thirty-two bytes")
//@[strip] run-call-fail: requireConstant 0 => 0x

// Constant messages longer than one word are dropped by `--revert-strings strip` like short ones.
contract LongStrings {
    string constant MESSAGE = "a constant message that is longer than thirty-two bytes";

    function requireLong(uint256 x) external pure {
        require(x != 0, "a require message that is longer than thirty-two bytes");
    }

    function revertLong() external pure {
        revert("a revert message that is longer than thirty-two bytes");
    }

    function requireConstant(uint256 x) external pure {
        require(x != 0, MESSAGE);
    }
}
