//@ revisions: default strip debug
//@[strip] compile-flags: --revert-strings strip
//@[debug] compile-flags: --revert-strings debug

//@ run-call: passthrough 7 => 7
//@[default,strip] run-call-fail: passthrough 7; value=1 => 0x
//@[debug] run-call-fail: passthrough 7; value=1 => Error("Ether sent to non-payable function")

// An unknown selector, and a plain Ether transfer that `receive` accepts.
//@[default,strip] run-call-fail: 0xdeadbeef => 0x
//@[debug] run-call-fail: 0xdeadbeef => Error("Unknown signature and no fallback defined")
//@ run-call: 0x; value=1

// With a `receive` function, an unmatched selector reverts with empty data by default and
// with `strip`, and reports "Unknown signature and no fallback defined" with `debug`, like
// solc. Plain Ether transfers succeed in every mode.
contract Receive {
    receive() external payable {}

    function passthrough(uint256 x) external pure returns (uint256) {
        return x;
    }
}
