contract DirtyStorage {
    uint8 a; uint8 b; int16 c; bool d; address e; bytes4 f; uint64 g;
    uint8[] arr;
    struct S { uint8 x; int8 y; bool z; }
    S s;
    mapping(uint256 => uint8) m;
    E en;
    enum E { A, B, C }

    function slotWrite(uint256 raw) internal { assembly { sstore(a.slot, raw) } }

    function readA(uint256 raw) external returns (uint8) { slotWrite(raw); return a; }
    function readB(uint256 raw) external returns (uint8) { slotWrite(raw); return b; }
    function readC(uint256 raw) external returns (int16) { slotWrite(raw); return c; }
    function readD(uint256 raw) external returns (bool) { slotWrite(raw); return d; }
    function readE(uint256 raw) external returns (address) { slotWrite(raw); return e; }
    function readF(uint256 raw) external returns (bytes4) { slotWrite(raw); return f; }
    function readG(uint256 raw) external returns (uint64) { slotWrite(raw); return g; }
    function readAll(uint256 raw) external returns (uint8, uint8, int16, bool, address, bytes4, uint64) { slotWrite(raw); return (a, b, c, d, e, f, g); }
    function writeAAfterDirty(uint256 raw, uint256 v) external returns (uint256 slot) { slotWrite(raw); uint8 x; assembly { x := v } a = x; assembly { slot := sload(a.slot) } }
    function writeCAfterDirty(uint256 raw, uint256 v) external returns (uint256 slot) { slotWrite(raw); int16 x; assembly { x := v } c = x; assembly { slot := sload(a.slot) } }
    function writeDAfterDirty(uint256 raw, uint256 v) external returns (uint256 slot) { slotWrite(raw); bool x; assembly { x := v } d = x; assembly { slot := sload(a.slot) } }
    function writeFAfterDirty(uint256 raw, uint256 v) external returns (uint256 slot) { slotWrite(raw); bytes4 x; assembly { x := v } f = x; assembly { slot := sload(a.slot) } }
    function incA(uint256 raw) external returns (uint8, uint256 slot) { slotWrite(raw); a++; uint8 v = a; assembly { slot := sload(a.slot) } return (v, slot); }
    function incAUnchecked(uint256 raw) external returns (uint8, uint256 slot) { slotWrite(raw); unchecked { a++; } uint8 v = a; assembly { slot := sload(a.slot) } return (v, slot); }
    function compoundC(uint256 raw) external returns (int16, uint256 slot) { slotWrite(raw); c -= 1; int16 v = c; assembly { slot := sload(a.slot) } return (v, slot); }
    function deleteA(uint256 raw) external returns (uint256 slot) { slotWrite(raw); delete a; assembly { slot := sload(a.slot) } }
    function cmpA(uint256 raw) external returns (bool) { slotWrite(raw); return a == 1; }
    function cmpC(uint256 raw) external returns (bool) { slotWrite(raw); return c < 0; }
    function cmpAB(uint256 raw) external returns (bool) { slotWrite(raw); return a == b; }
    function widenA(uint256 raw) external returns (uint256) { slotWrite(raw); return a; }
    function widenC(uint256 raw) external returns (int256) { slotWrite(raw); return c; }
    function structRead(uint256 raw) external returns (uint8, int8, bool) { assembly { sstore(s.slot, raw) } return (s.x, s.y, s.z); }
    function structCopy(uint256 raw) external returns (uint8, int8, bool) { assembly { sstore(s.slot, raw) } S memory t = s; return (t.x, t.y, t.z); }
    function structEncode(uint256 raw) external returns (bytes memory) { assembly { sstore(s.slot, raw) } return abi.encode(s); }
    function enumRead(uint256 raw) external returns (E) { assembly { sstore(en.slot, raw) } return en; }
    function enumReadToUint(uint256 raw) external returns (uint256) { assembly { sstore(en.slot, raw) } return uint256(en); }
    function enumReadEq(uint256 raw) external returns (bool) { assembly { sstore(en.slot, raw) } return en == E.B; }
    function arrRead(uint256 raw) external returns (uint8, uint8) { arr.push(1); arr.push(2); assembly { mstore(0, arr.slot) sstore(keccak256(0, 0x20), raw) } return (arr[0], arr[1]); }
    function arrCopy(uint256 raw) external returns (uint8[] memory) { arr.push(1); arr.push(2); assembly { mstore(0, arr.slot) sstore(keccak256(0, 0x20), raw) } return arr; }
    function arrPushAfterDirty(uint256 raw) external returns (uint8, uint256 w) { arr.push(1); assembly { mstore(0, arr.slot) sstore(keccak256(0, 0x20), raw) } arr.push(3); uint8 v = arr[1]; assembly { mstore(0, arr.slot) w := sload(keccak256(0, 0x20)) } return (v, w); }
    function arrPop(uint256 raw) external returns (uint256 w) { arr.push(1); arr.push(2); assembly { mstore(0, arr.slot) sstore(keccak256(0, 0x20), raw) } arr.pop(); assembly { mstore(0, arr.slot) w := sload(keccak256(0, 0x20)) } }
    function mapRead(uint256 raw) external returns (uint8) { assembly { mstore(0, 1) mstore(0x20, m.slot) sstore(keccak256(0, 0x40), raw) } return m[1]; }
}
