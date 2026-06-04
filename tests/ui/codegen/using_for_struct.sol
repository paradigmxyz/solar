//@ignore-host: windows
//@compile-flags: --emit=mir

// `using L for S` attached calls on a struct (a reference type) must resolve to
// the library function — previously the receiver's `Ref(Struct, loc)` type
// failed the using-directive match, so `s.hashLib()` was lowered as an external
// CALL (returning a memory pointer) instead of inlining the keccak.
// Runtime-verified. Regression for nitro's `getStartMachineHash`.
struct S {
    bytes32 a;
    uint32 i;
    uint256 j;
    bytes32 b;
}

library L {
    function hashLib(S memory s) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked("x", s.a, s.i, s.j, s.b));
    }
}

contract C {
    using L for S;

    function viaMethod(bytes32 a, bytes32 b) public pure returns (bytes32) {
        S memory s = S({a: a, i: 0, j: 0, b: b});
        return s.hashLib();
    }
}
