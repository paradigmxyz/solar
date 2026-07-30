//@run-call: runtimeFunction() => 68
//@run-call: bytesObjectSelector() => 0xa7a0d537

// `abi.encodeCall(F, (args))` takes the selector from the function reference
// and ABI-encodes the argument tuple after it, both as a `bytes memory` value
// and directly as low-level `.call` data. Verified byte-exact against solc.

interface IERC20 {
    function transfer(address, uint256) external returns (bool);
}

contract AbiEncodeCall {
    // AEC-LABEL: fn @asValue
    // AEC: 0xa9059cbb
    function asValue(address to, uint256 v) public pure returns (bytes memory) {
        return abi.encodeCall(IERC20.transfer, (to, v));
    }

    // AEC-LABEL: fn @viaCall
    // AEC: call
    function viaCall(address token, address to, uint256 v) public returns (bool ok) {
        (ok, ) = token.call(abi.encodeCall(IERC20.transfer, (to, v)));
    }

    function runtimeFunction() public view returns (uint256) {
        function(address, uint256) external returns (bool) fn = IERC20(address(this)).transfer;
        bytes memory data = abi.encodeCall(fn, (address(0), 1));
        return data.length;
    }

    function something() external pure {}

    function bytesObjectSelector() external view returns (bytes4) {
        bytes memory data = abi.encodeCall(this.something, ());
        return bytes4(data);
    }
}
