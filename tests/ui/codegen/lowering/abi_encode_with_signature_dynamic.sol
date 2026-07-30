//@compile-flags: -Zcodegen --emit=bin-runtime
// ported-from: test/utils/mocks/MockReentrancyGuard.sol

// `abi.encodeWithSignature` with a signature that is not a string literal.
// A conditional between literals selects between the two constant selectors,
// so the common `cond ? "f(uint256)" : "g(uint256)"` needs no runtime hash;
// any other string is hashed at runtime and truncated to its leading four
// bytes. Both the low-level call-data path and the `bytes memory` value path
// go through the same resolution.
contract AbiEncodeWithSignatureDynamic {
    function callCond(address t, bool guarded, uint256 v) external returns (bool ok) {
        (ok, ) = t.call(abi.encodeWithSignature(guarded ? "fa(uint256)" : "fb(uint256)", v));
    }

    function encodeCond(bool guarded, uint256 v) external pure returns (bytes memory) {
        return abi.encodeWithSignature(guarded ? "fa(uint256)" : "fb(uint256)", v);
    }

    function encodeLiteral(uint256 v) external pure returns (bytes memory) {
        return abi.encodeWithSignature("fa(uint256)", v);
    }

    function encodeRuntime(string memory sig, uint256 v) external pure returns (bytes memory) {
        return abi.encodeWithSignature(sig, v);
    }
}
