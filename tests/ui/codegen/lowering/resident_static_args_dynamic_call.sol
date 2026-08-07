//@ compile-flags: -Ogas
//@ run-call: first 21 => 42
//@ run-call: second 21 => 42

contract ResidentStaticArgsDynamicCall {
    function first(uint256 value) external pure returns (uint256) {
        uint256[1] memory values = [value];
        return readAcrossDynamicCall(values);
    }

    function second(uint256 value) external pure returns (uint256) {
        uint256[1] memory values = [value];
        return readAcrossDynamicCall(values);
    }

    // The recursive callee requires a dynamic frame. Keep the static caller's
    // resident memory pointer below its return address so the second load can
    // still use the pointer after the call.
    function readAcrossDynamicCall(uint256[1] memory values) internal pure returns (uint256) {
        uint256 firstValue = values[0];
        recurse(0);
        return firstValue + values[0];
    }

    function recurse(uint256 depth) private pure {
        if (depth != 0) recurse(depth - 1);
    }
}
