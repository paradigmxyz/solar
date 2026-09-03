contract DirtyBoolAddressBytes {
    bool sb;
    address sa;
    bytes4 s4;
    uint64 pad;

    function injectBool(uint256 raw) internal pure returns (bool x) { assembly { x := raw } }
    function injectAddr(uint256 raw) internal pure returns (address x) { assembly { x := raw } }
    function inject4(uint256 raw) internal pure returns (bytes4 x) { assembly { x := raw } }

    function boolRet(uint256 raw) external pure returns (bool) { return injectBool(raw); }
    function boolEq(uint256 raw, uint256 raw2) external pure returns (bool) { return injectBool(raw) == injectBool(raw2); }
    function boolEqTrue(uint256 raw) external pure returns (bool) { return injectBool(raw) == true; }
    function boolNot(uint256 raw) external pure returns (bool) { return !injectBool(raw); }
    function boolIf(uint256 raw) external pure returns (uint256) { if (injectBool(raw)) return 1; return 0; }
    function boolAnd(uint256 raw, uint256 raw2) external pure returns (bool) { return injectBool(raw) && injectBool(raw2); }
    function boolXor(uint256 raw, uint256 raw2) external pure returns (bool) { return injectBool(raw) != injectBool(raw2); }
    function boolAssembly(uint256 raw) external pure returns (uint256 r) { bool b = injectBool(raw); assembly { r := b } }
    function boolStore(uint256 raw) external returns (bool, uint256 slot) { sb = injectBool(raw); bool v = sb; assembly { slot := sload(sb.slot) } return (v, slot); }
    function boolMemory(uint256 raw) external pure returns (bool, uint256 w) { bool[1] memory a; a[0] = injectBool(raw); assembly { w := mload(a) } return (a[0], w); }
    function boolTernary(uint256 raw) external pure returns (uint256) { return injectBool(raw) ? 1 : 2; }
    function boolEncode(uint256 raw) external pure returns (bytes memory) { return abi.encode(injectBool(raw)); }

    function addrRet(uint256 raw) external pure returns (address) { return injectAddr(raw); }
    function addrEq(uint256 raw, uint256 raw2) external pure returns (bool) { return injectAddr(raw) == injectAddr(raw2); }
    function addrLt(uint256 raw, uint256 raw2) external pure returns (bool) { return injectAddr(raw) < injectAddr(raw2); }
    function addrToUint(uint256 raw) external pure returns (uint256) { return uint256(uint160(injectAddr(raw))); }
    function addrToBytes20(uint256 raw) external pure returns (bytes20) { return bytes20(injectAddr(raw)); }
    function addrAssembly(uint256 raw) external pure returns (uint256 r) { address a = injectAddr(raw); assembly { r := a } }
    function addrStore(uint256 raw) external returns (address, uint256 slot) { sa = injectAddr(raw); pad = 7; address v = sa; assembly { slot := sload(sa.slot) } return (v, slot); }
    function addrEncodePacked(uint256 raw) external pure returns (bytes memory) { return abi.encodePacked(injectAddr(raw)); }
    function addrIsZero(uint256 raw) external pure returns (bool) { return injectAddr(raw) == address(0); }
    function addrPayable(uint256 raw) external pure returns (address payable) { return payable(injectAddr(raw)); }

    function b4Ret(uint256 raw) external pure returns (bytes4) { return inject4(raw); }
    function b4Eq(uint256 raw, uint256 raw2) external pure returns (bool) { return inject4(raw) == inject4(raw2); }
    function b4Lt(uint256 raw, uint256 raw2) external pure returns (bool) { return inject4(raw) < inject4(raw2); }
    function b4ToUint32(uint256 raw) external pure returns (uint32) { return uint32(inject4(raw)); }
    function b4ToBytes2(uint256 raw) external pure returns (bytes2) { return bytes2(inject4(raw)); }
    function b4ToBytes8(uint256 raw) external pure returns (bytes8) { return bytes8(inject4(raw)); }
    function b4ToBytes32(uint256 raw) external pure returns (bytes32) { return bytes32(inject4(raw)); }
    function b4Index(uint256 raw, uint256 i) external pure returns (bytes1) { return inject4(raw)[i]; }
    function b4Shl(uint256 raw, uint256 n) external pure returns (bytes4) { return inject4(raw) << uint8(n); }
    function b4Shr(uint256 raw, uint256 n) external pure returns (bytes4) { return inject4(raw) >> uint8(n); }
    function b4And(uint256 raw, uint256 raw2) external pure returns (bytes4) { return inject4(raw) & inject4(raw2); }
    function b4Or(uint256 raw, uint256 raw2) external pure returns (bytes4) { return inject4(raw) | inject4(raw2); }
    function b4Not(uint256 raw) external pure returns (bytes4) { return ~inject4(raw); }
    function b4Assembly(uint256 raw) external pure returns (uint256 r) { bytes4 b = inject4(raw); assembly { r := b } }
    function b4Store(uint256 raw) external returns (bytes4, uint256 slot) { pad = 3; s4 = inject4(raw); bytes4 v = s4; assembly { slot := sload(s4.slot) } return (v, slot); }
    function b4Memory(uint256 raw) external pure returns (bytes4, uint256 w) { bytes4[1] memory a; a[0] = inject4(raw); assembly { w := mload(a) } return (a[0], w); }
    function b4EncodePacked(uint256 raw) external pure returns (bytes memory) { return abi.encodePacked(inject4(raw)); }
    function b4Length(uint256 raw) external pure returns (uint256) { return inject4(raw).length; }
    function b4Concat(uint256 raw) external pure returns (bytes memory) { return bytes.concat(inject4(raw), inject4(raw)); }
}
