//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: selectedBranch(bool) true => 1
//@ run-call: selectedBranch(bool) false => 2
// ported-from: test/utils/mocks/MockReentrancyGuard.sol

// `abi.encodeWithSignature` with a signature that is not a string literal.
// A conditional between literals selects between the two constant selectors,
// so the common `cond ? "f(uint256)" : "g(uint256)"` needs no runtime hash;
// any other string is hashed at runtime and truncated to its leading four
// bytes. Both the low-level call-data path and the `bytes memory` value path
// go through the same resolution.
contract AbiEncodeWithSignatureDynamic {
    uint256 marker;

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

    function firstSignature() internal returns (string memory) {
        marker = 1;
        return "first()";
    }

    function secondSignature() internal returns (string memory) {
        marker = 2;
        return "second()";
    }

    function selectedBranch(bool first) external returns (uint256) {
        abi.encodeWithSignature(first ? firstSignature() : secondSignature());
        return marker;
    }
}
