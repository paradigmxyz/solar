type Small is uint8;
type Signed is int16;
type Fixed is bytes2;

using {addSmall as +, eqSmall as ==} for Small global;

function addSmall(Small a, Small b) pure returns (Small r) {
    assembly { r := add(a, b) }
}

function eqSmall(Small a, Small b) pure returns (bool) {
    return Small.unwrap(a) == Small.unwrap(b);
}

contract DirtyEnumUdvtFn {
    enum E { A, B, C }
    E se;
    Small ss;

    function injectE(uint256 raw) internal pure returns (E x) { assembly { x := raw } }
    function injectSmall(uint256 raw) internal pure returns (Small x) { assembly { x := raw } }
    function injectSigned(uint256 raw) internal pure returns (Signed x) { assembly { x := raw } }
    function injectFixed(uint256 raw) internal pure returns (Fixed x) { assembly { x := raw } }

    function enumRet(uint256 raw) external pure returns (E) { return injectE(raw); }
    function enumToUint(uint256 raw) external pure returns (uint256) { return uint256(injectE(raw)); }
    function enumToUint8(uint256 raw) external pure returns (uint8) { return uint8(injectE(raw)); }
    function enumEq(uint256 raw) external pure returns (bool) { return injectE(raw) == E.B; }
    function enumLt(uint256 raw) external pure returns (bool) { return injectE(raw) < E.C; }
    function enumFromUint(uint256 raw) external pure returns (E) { return E(raw); }
    function enumFromUint8(uint256 raw) external pure returns (E) { uint8 x; assembly { x := raw } return E(x); }
    function enumStore(uint256 raw) external returns (E, uint256 slot) { se = injectE(raw); E v = se; assembly { slot := sload(se.slot) } return (v, slot); }
    function enumMemory(uint256 raw) external pure returns (E) { E[1] memory a; a[0] = injectE(raw); return a[0]; }
    function enumAssembly(uint256 raw) external pure returns (uint256 r) { E e = injectE(raw); assembly { r := e } }
    function enumEncode(uint256 raw) external pure returns (bytes memory) { return abi.encode(injectE(raw)); }
    function enumSwitch(uint256 raw) external pure returns (uint256) { E e = injectE(raw); if (e == E.A) return 1; if (e == E.B) return 2; if (e == E.C) return 3; return 4; }

    function smallRet(uint256 raw) external pure returns (Small) { return injectSmall(raw); }
    function smallUnwrap(uint256 raw) external pure returns (uint8) { return Small.unwrap(injectSmall(raw)); }
    function smallUnwrapWiden(uint256 raw) external pure returns (uint256) { return Small.unwrap(injectSmall(raw)); }
    function smallWrap(uint256 raw) external pure returns (Small) { uint8 x; assembly { x := raw } return Small.wrap(x); }
    function smallAdd(uint256 raw, uint256 raw2) external pure returns (Small) { return injectSmall(raw) + injectSmall(raw2); }
    function smallAddUnwrap(uint256 raw, uint256 raw2) external pure returns (uint256) { return Small.unwrap(injectSmall(raw) + injectSmall(raw2)); }
    function smallEq(uint256 raw, uint256 raw2) external pure returns (bool) { return injectSmall(raw) == injectSmall(raw2); }
    function smallStore(uint256 raw) external returns (Small, uint256 slot) { ss = injectSmall(raw); Small v = ss; assembly { slot := sload(ss.slot) } return (v, slot); }
    function smallMemory(uint256 raw) external pure returns (Small, uint256 w) { Small[1] memory a; a[0] = injectSmall(raw); assembly { w := mload(a) } return (a[0], w); }
    function smallEncode(uint256 raw) external pure returns (bytes memory) { return abi.encode(injectSmall(raw)); }

    function signedRet(uint256 raw) external pure returns (Signed) { return injectSigned(raw); }
    function signedUnwrapWiden(uint256 raw) external pure returns (int256) { return Signed.unwrap(injectSigned(raw)); }
    function signedNeg(uint256 raw) external pure returns (int16) { return -Signed.unwrap(injectSigned(raw)); }
    function fixedRet(uint256 raw) external pure returns (Fixed) { return injectFixed(raw); }
    function fixedUnwrapToUint(uint256 raw) external pure returns (uint16) { return uint16(Fixed.unwrap(injectFixed(raw))); }

    function target() external pure {}
    function fnRet(uint256 raw) external view returns (uint256 addr_, uint256 sel_) {
        function() external f = this.target;
        assembly { f.address := raw f.selector := raw }
        address a = f.address; bytes4 s = f.selector;
        assembly { addr_ := a sel_ := s }
    }
    function fnEqDirty(uint256 raw) external view returns (bool) {
        function() external f = this.target; function() external g = this.target;
        assembly { f.address := or(raw, address()) }
        return f == g;
    }
    function fnSelector(uint256 raw) external view returns (bytes4) {
        function() external f = this.target;
        assembly { f.selector := raw }
        return f.selector;
    }
}
