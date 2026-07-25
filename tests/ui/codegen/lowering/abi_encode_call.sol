//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=AEC

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
}
