contract StoragePacking {
    uint8 a; uint16 b; bool c; int8 d; bytes4 e; address f; uint64 g;
    uint128 h; uint128 i;
    uint256 j;
    struct P { uint8 x; int16 y; bool z; bytes3 w; }
    P p;
    P[] ps;
    uint8[] u8s;
    bool[] bs;
    int8[] i8s;
    bytes1[] b1s;
    uint8[40] fixedU8;
    bytes3[11] fixedB3;
    mapping(uint256 => P) mp;
    enum E { A, B, C }
    E en; uint8 afterEn;
    function writeRead(uint8 va, uint16 vb, bool vc, int8 vd, bytes4 ve, address vf, uint64 vg) external returns (uint8, uint16, bool, int8, bytes4, address, uint64) {
        a = va; b = vb; c = vc; d = vd; e = ve; f = vf; g = vg;
        return (a, b, c, d, e, f, g);
    }
    function slot0(uint8 va, uint16 vb, bool vc, int8 vd, bytes4 ve, address vf, uint64 vg) external returns (uint256 r) {
        a = va; b = vb; c = vc; d = vd; e = ve; f = vf; g = vg;
        assembly { r := sload(0) }
    }
    function overwrite(uint8 va, uint8 va2, uint16 vb) external returns (uint256 r, uint8, uint16) {
        a = va; b = vb; a = va2;
        assembly { r := sload(0) }
        return (r, a, b);
    }
    function halves(uint128 vh, uint128 vi) external returns (uint256 r, uint128, uint128) {
        h = vh; i = vi;
        assembly { r := sload(1) }
        return (r, h, i);
    }
    function compound(uint8 va, uint8 delta) external returns (uint8, uint16) {
        a = va; b = 5; a += delta; b -= 1; return (a, b);
    }
    function compoundU(uint8 va, uint8 delta) external returns (uint8) {
        a = va; unchecked { a += delta; } return a;
    }
    function incDec(uint8 va) external returns (uint8, uint8, uint8) {
        a = va; uint8 x = a++; uint8 y = ++a; return (x, y, a);
    }
    function decUnder(uint8 va) external returns (uint8) { a = va; a--; return a; }
    function negD(int8 vd) external returns (int8) { d = vd; d = -d; return d; }
    function structPack(uint8 x, int16 y, bool z, bytes3 w) external returns (uint256 r, uint8, int16, bool, bytes3) {
        p = P(x, y, z, w);
        assembly { r := sload(p.slot) }
        return (r, p.x, p.y, p.z, p.w);
    }
    function structMember(uint8 x, int16 y) external returns (uint256 r) {
        p.x = x; p.y = y; p.z = true; p.w = 0xabcdef;
        assembly { r := sload(p.slot) }
    }
    function structDelete(uint8 x, int16 y) external returns (uint256 r, uint8) {
        p.x = x; p.y = y; p.z = true; p.w = 0xabcdef;
        delete p;
        assembly { r := sload(p.slot) }
        return (r, p.x);
    }
    function structMemberDelete(uint8 x, int16 y) external returns (uint256 r) {
        p.x = x; p.y = y; p.z = true; p.w = 0xabcdef;
        delete p.y;
        assembly { r := sload(p.slot) }
    }
    function structArr(uint8 x, int16 y) external returns (uint256 r0, uint256 r1, int16) {
        ps.push(P(x, y, true, 0x010203));
        ps.push(P(x, -y, false, 0x040506));
        ps[0].y += 1;
        uint256 s; assembly { mstore(0, ps.slot) s := keccak256(0, 32) }
        assembly { r0 := sload(s) r1 := sload(add(s, 1)) }
        return (r0, r1, ps[0].y);
    }
    function u8Array(uint8 v, uint256 n) external returns (uint256 r, uint256 len, uint8 last) {
        require(n <= 40);
        for (uint256 k; k < n; k++) u8s.push(uint8(v + k));
        uint256 s; assembly { mstore(0, u8s.slot) s := keccak256(0, 32) }
        assembly { r := sload(s) }
        return (r, u8s.length, n > 0 ? u8s[n - 1] : 0);
    }
    function u8ArrayPop(uint8 v) external returns (uint256 r, uint256 len) {
        for (uint256 k; k < 33; k++) u8s.push(v);
        u8s.pop(); u8s.pop();
        uint256 s; assembly { mstore(0, u8s.slot) s := keccak256(0, 32) }
        assembly { r := sload(add(s, 1)) }
        return (r, u8s.length);
    }
    function u8ArrayPopClears(uint8 v) external returns (uint256 r) {
        for (uint256 k; k < 3; k++) u8s.push(v);
        u8s.pop();
        uint256 s; assembly { mstore(0, u8s.slot) s := keccak256(0, 32) }
        assembly { r := sload(s) }
    }
    function u8ArrayWrite(uint8 v, uint256 idx) external returns (uint256 r) {
        for (uint256 k; k < 35; k++) u8s.push(0);
        u8s[idx] = v;
        u8s[idx] += 1;
        uint256 s; assembly { mstore(0, u8s.slot) s := keccak256(0, 32) }
        assembly { r := sload(add(s, div(idx, 32))) }
    }
    function u8ArrayDelete(uint8 v) external returns (uint256 r, uint256 len) {
        for (uint256 k; k < 35; k++) u8s.push(v);
        delete u8s;
        uint256 s; assembly { mstore(0, u8s.slot) s := keccak256(0, 32) }
        assembly { r := sload(add(s, 1)) }
        return (r, u8s.length);
    }
    function boolArray(bool v) external returns (uint256 r, bool) {
        bs.push(v); bs.push(!v); bs.push(v);
        uint256 s; assembly { mstore(0, bs.slot) s := keccak256(0, 32) }
        assembly { r := sload(s) }
        return (r, bs[1]);
    }
    function i8Array(int8 v) external returns (uint256 r, int8, int256) {
        i8s.push(v); i8s.push(-v); i8s.push(int8(-1));
        uint256 s; assembly { mstore(0, i8s.slot) s := keccak256(0, 32) }
        assembly { r := sload(s) }
        return (r, i8s[1], i8s[2]);
    }
    function b1Array(bytes1 v) external returns (uint256 r, bytes1) {
        b1s.push(v); b1s.push(~v);
        uint256 s; assembly { mstore(0, b1s.slot) s := keccak256(0, 32) }
        assembly { r := sload(s) }
        return (r, b1s[1]);
    }
    function fixedU8Arr(uint8 v, uint256 idx) external returns (uint256 r0, uint256 r1, uint8) {
        fixedU8[idx] = v;
        fixedU8[39] = 0xff;
        assembly { r0 := sload(fixedU8.slot) r1 := sload(add(fixedU8.slot, 1)) }
        return (r0, r1, fixedU8[idx]);
    }
    function fixedB3Arr(bytes3 v, uint256 idx) external returns (uint256 r0, uint256 r1, bytes3) {
        fixedB3[idx] = v;
        fixedB3[10] = 0xffffff;
        assembly { r0 := sload(fixedB3.slot) r1 := sload(add(fixedB3.slot, 1)) }
        return (r0, r1, fixedB3[idx]);
    }
    function fixedDelete(uint8 v) external returns (uint256 r0, uint256 r1) {
        fixedU8[0] = v; fixedU8[39] = v;
        delete fixedU8;
        assembly { r0 := sload(fixedU8.slot) r1 := sload(add(fixedU8.slot, 1)) }
    }
    function mapStruct(uint256 k, uint8 x, int16 y) external returns (uint8, int16, bool) {
        mp[k] = P(x, y, x > 3, 0);
        mp[k].x += 1;
        P storage q = mp[k];
        q.y -= 1;
        return (mp[k].x, q.y, mp[k].z);
    }
    function enumPack(uint8 v) external returns (uint256 r, E) {
        require(v < 3);
        en = E(v); afterEn = 0xaa;
        assembly { r := sload(en.slot) }
        return (r, en);
    }
    function memCopyStruct(uint8 x, int16 y) external returns (uint8, int16, bool, bytes3) {
        p = P(x, y, true, 0xabcdef);
        P memory m = p;
        m.x += 1;
        p = m;
        return (p.x, p.y, p.z, p.w);
    }
    function structTupleAssign(uint8 x, int16 y) external returns (uint8, int16) {
        (p.x, p.y) = (x, y);
        (p.x, p.y) = (uint8(uint16(p.y)), int16(uint16(p.x)));
        return (p.x, p.y);
    }
    function readWrongWidth(uint8 va, uint16 vb) external returns (uint256 r) {
        a = va; b = vb;
        assembly { r := and(shr(8, sload(0)), 0xffff) }
    }
    function dirtySlot(uint256 raw) external returns (uint8, uint16, bool, int8, bytes4, address, uint64) {
        assembly { sstore(0, raw) }
        return (a, b, c, d, e, f, g);
    }
    function dirtySlotWrite(uint256 raw, uint8 va) external returns (uint256 r) {
        assembly { sstore(0, raw) }
        a = va;
        assembly { r := sload(0) }
    }
    function dirtyStruct(uint256 raw) external returns (uint8, int16, bool, bytes3) {
        assembly { sstore(p.slot, raw) }
        return (p.x, p.y, p.z, p.w);
    }
    function dirtyStructCopy(uint256 raw) external returns (uint8, int16, bool, bytes3) {
        assembly { sstore(p.slot, raw) }
        P memory m = p;
        return (m.x, m.y, m.z, m.w);
    }
    function dirtyStructEnc(uint256 raw) external returns (bytes memory) {
        assembly { sstore(p.slot, raw) }
        return abi.encode(p);
    }
    function dirtyU8Arr(uint256 raw, uint256 idx) external returns (uint8, uint256) {
        require(idx < 32);
        for (uint256 k; k < 32; k++) u8s.push(0);
        uint256 s; assembly { mstore(0, u8s.slot) s := keccak256(0, 32) sstore(s, raw) }
        return (u8s[idx], u8s.length);
    }
    function dirtyEnum(uint256 raw) external returns (E) {
        assembly { sstore(en.slot, raw) }
        return en;
    }
    function dirtyEnumIdx(uint256 raw) external returns (uint8) {
        assembly { sstore(en.slot, raw) }
        return uint8(en);
    }
    function dirtyBoolArr(uint256 raw) external returns (bool, bool) {
        bs.push(false); bs.push(false);
        uint256 s; assembly { mstore(0, bs.slot) s := keccak256(0, 32) sstore(s, raw) }
        return (bs[0], bs[1]);
    }
}
