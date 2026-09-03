type Small is uint8;
type SignedSmall is int8;

function eqSmallFree(Small a, Small b) pure returns (bool) { return Small.unwrap(a) == Small.unwrap(b); }

contract DirtyParam {
    function inject(uint256 raw) internal pure returns (Small x) { assembly { x := raw } }
    function injectU8(uint256 raw) internal pure returns (uint8 x) { assembly { x := raw } }
    function injectI8(uint256 raw) internal pure returns (SignedSmall x) { assembly { x := raw } }
    function injectB(uint256 raw) internal pure returns (bool x) { assembly { x := raw } }

    function eqU8(uint8 a, uint8 b) internal pure returns (bool) { return a == b; }
    function eqSmall(Small a, Small b) internal pure returns (bool) { return Small.unwrap(a) == Small.unwrap(b); }
    function eqSmallPub(Small a, Small b) public pure returns (bool) { return Small.unwrap(a) == Small.unwrap(b); }
    function ltSigned(SignedSmall a, SignedSmall b) internal pure returns (bool) { return SignedSmall.unwrap(a) < SignedSmall.unwrap(b); }
    function widenSmall(Small a) internal pure returns (uint256) { return Small.unwrap(a); }
    function widenU8(uint8 a) internal pure returns (uint256) { return a; }
    function widenSigned(SignedSmall a) internal pure returns (int256) { return SignedSmall.unwrap(a); }
    function andBool(bool a, bool b) internal pure returns (bool) { return a == b; }
    function unwrapU8(Small a) internal pure returns (uint8) { return Small.unwrap(a); }

    function callEqU8(uint256 raw, uint256 raw2) external pure returns (bool) { return eqU8(injectU8(raw), injectU8(raw2)); }
    function callEqSmall(uint256 raw, uint256 raw2) external pure returns (bool) { return eqSmall(inject(raw), inject(raw2)); }
    function callEqSmallFree(uint256 raw, uint256 raw2) external pure returns (bool) { return eqSmallFree(inject(raw), inject(raw2)); }
    function callEqSmallPub(uint256 raw, uint256 raw2) external pure returns (bool) { return eqSmallPub(inject(raw), inject(raw2)); }
    function callLtSigned(uint256 raw, uint256 raw2) external pure returns (bool) { return ltSigned(injectI8(raw), injectI8(raw2)); }
    function callWidenSmall(uint256 raw) external pure returns (uint256) { return widenSmall(inject(raw)); }
    function callWidenU8(uint256 raw) external pure returns (uint256) { return widenU8(injectU8(raw)); }
    function callWidenSigned(uint256 raw) external pure returns (int256) { return widenSigned(injectI8(raw)); }
    function callAndBool(uint256 raw, uint256 raw2) external pure returns (bool) { return andBool(injectB(raw), injectB(raw2)); }
    function callUnwrapThenCmp(uint256 raw) external pure returns (bool) { return unwrapU8(inject(raw)) == 1; }
    function callUnwrapThenWiden(uint256 raw) external pure returns (uint256) { return uint256(unwrapU8(inject(raw))); }
    function callUnwrapThenAssembly(uint256 raw) external pure returns (uint256 r) { uint8 v = unwrapU8(inject(raw)); assembly { r := v } }
    function localSmallCmp(uint256 raw, uint256 raw2) external pure returns (bool) { Small a = inject(raw); Small b = inject(raw2); return Small.unwrap(a) == Small.unwrap(b); }
}
