//@ codegen-matrix: standard
//@ compile-flags: --revert-strings debug
//@ run-call: passthrough 7 => 7
//@ run-call-fail: passthrough 7; value=1 => Error("Ether sent to non-payable function")
//@ run-call-fail: 0xdeadbeef => Error("Unknown signature and no fallback defined")
//@ run-call-fail: 0x8f336a56 => Error("ABI decoding: tuple data too short")
//@ run-call: slice 0x01020304, 1, 3 => 2
//@ run-call-fail: slice 0x01020304, 0, 5 => Error("Slice is greater than length")
//@ run-call-fail: slice 0x01020304, 2, 1 => Error("Slice starts after end")
//@ run-call-fail: callNoCode => Error("Target contract does not contain code")
//@ run-call-fail: userRevert => Error("user")
//@ run-call-fail: userRequire 0 => Error("x must be nonzero")

// `--revert-strings debug` encodes solc's messages for compiler-generated reverts as
// `Error(string)` payloads: rejected Ether, unknown selectors, short calldata, invalid
// calldata slices, and calls to code-less targets. User-supplied reason strings are kept.
interface Target {
    function ping() external;
}

contract RevertStringsDebug {
    function passthrough(uint256 x) external pure returns (uint256) {
        return x;
    }

    function slice(bytes calldata data, uint256 start, uint256 end) external pure returns (uint256) {
        bytes calldata sliced = data[start:end];
        return sliced.length;
    }

    function callNoCode() external {
        Target(address(0x1234)).ping();
    }

    function userRevert() external pure {
        revert("user");
    }

    function userRequire(uint256 x) external pure returns (uint256) {
        require(x != 0, "x must be nonzero");
        return x;
    }
}
