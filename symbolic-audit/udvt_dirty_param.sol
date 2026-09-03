// A user-defined value type over `uint8` whose word is dirty (assigned in
// assembly) is passed as a function argument. Inside the callee,
// `Small.unwrap(param)` is treated as already clean, so comparisons and
// widenings see the dirty bits. solc cleans at the comparison or widening.
// Plain `uint8` parameters and the same code written inline agree with solc.
type Small is uint8;
type SignedSmall is int8;

using {eqSmall as ==} for Small global;

function eqSmall(Small a, Small b) pure returns (bool) {
    return Small.unwrap(a) == Small.unwrap(b);
}

contract UdvtDirtyParam {
    function inject(uint256 raw) internal pure returns (Small x) {
        assembly {
            x := raw
        }
    }

    function injectSigned(uint256 raw) internal pure returns (SignedSmall x) {
        assembly {
            x := raw
        }
    }

    function injectU8(uint256 raw) internal pure returns (uint8 x) {
        assembly {
            x := raw
        }
    }

    function widenSmall(Small a) internal pure returns (uint256) {
        return Small.unwrap(a);
    }

    function widenSigned(SignedSmall a) internal pure returns (int256) {
        return SignedSmall.unwrap(a);
    }

    function eqU8(uint8 a, uint8 b) internal pure returns (bool) {
        return a == b;
    }

    // (0x100, 0): solc true, solar false.
    function viaOperator(uint256 raw, uint256 raw2) external pure returns (bool) {
        return inject(raw) == inject(raw2);
    }

    // (0x100, 0): solc true, solar false.
    function viaCall(uint256 raw, uint256 raw2) external pure returns (bool) {
        return eqSmall(inject(raw), inject(raw2));
    }

    // 0x101: solc 1, solar 0x101.
    function viaWiden(uint256 raw) external pure returns (uint256) {
        return widenSmall(inject(raw));
    }

    // 0x100: solc 0, solar 256.
    function viaWidenSigned(uint256 raw) external pure returns (int256) {
        return widenSigned(injectSigned(raw));
    }

    // Agrees with solc: the parameter type is a plain integer.
    function plainParam(uint256 raw, uint256 raw2) external pure returns (bool) {
        return eqU8(injectU8(raw), injectU8(raw2));
    }

    // Agrees with solc: no call boundary.
    function noCall(uint256 raw, uint256 raw2) external pure returns (bool) {
        return Small.unwrap(inject(raw)) == Small.unwrap(inject(raw2));
    }
}
