//@ codegen-matrix: standard
//@ compile-flags: --revert-strings debug
//@ run-call: passthrough 7 => 7
//@ run-call-fail: passthrough 7; value=1 => Error("Ether sent to non-payable function")
//@ run-call-fail: 0xdeadbeef => Error("Unknown signature and no fallback defined")
//@ run-call: 0x; value=1

// With a `receive` function, `--revert-strings debug` reports an unmatched selector as
// "Unknown signature and no fallback defined", like solc, while plain Ether transfers succeed.
contract RevertStringsDebugReceive {
    receive() external payable {}

    function passthrough(uint256 x) external pure returns (uint256) {
        return x;
    }
}
