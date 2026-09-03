contract YulAdvanced {
    function sw(uint256 x) external pure returns (uint256 r) { assembly { switch x case 0 { r := 10 } case 1 { r := 20 } case 0xff { r := 30 } default { r := 40 } } }
    function swNoDefault(uint256 x) external pure returns (uint256 r) { r = 7; assembly { switch x case 1 { r := 1 } case 2 { r := 2 } } }
    function swFallthrough(uint256 x) external pure returns (uint256 r) { assembly { switch lt(x, 5) case 1 { r := add(x, 100) } default { switch gt(x, 100) case 1 { r := 1 } default { r := 2 } } } }
    function forLoop(uint256 n) external pure returns (uint256 s) { assembly { for { let i := 0 } lt(i, n) { i := add(i, 1) } { if eq(i, 3) { continue } if gt(i, 6) { break } s := add(s, i) } } }
    function forNested(uint256 n) external pure returns (uint256 s) { require(n < 5); assembly { for { let i := 0 } lt(i, n) { i := add(i, 1) } { for { let j := 0 } lt(j, n) { j := add(j, 1) } { if eq(j, 2) { break } s := add(s, mul(i, j)) } } } }
    function yulFn(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { function f(x, y) -> z { z := add(mul(x, 2), y) } function g(x) -> p, q { p := x q := mul(x, x) } let p, q := g(b) r := f(a, add(p, q)) } }
    function yulLeave(uint256 a) external pure returns (uint256 r) { assembly { function f(x) -> z { z := 1 if gt(x, 5) { z := 2 leave } z := 3 } r := f(a) } }
    function yulRecur(uint256 n) external pure returns (uint256 r) { require(n < 10); assembly { function fact(x) -> z { z := 1 if gt(x, 1) { z := mul(x, fact(sub(x, 1))) } } r := fact(n) } }
    function letShadow(uint256 a) external pure returns (uint256 r) { assembly { let x := a { let y := add(x, 1) r := y } { let y := add(x, 2) r := add(r, y) } } }
    function multiAssign(uint256 a) external pure returns (uint256 x, uint256 y) { assembly { function two(v) -> p, q { p := v q := add(v, 1) } x, y := two(a) } }
    function byteOp(uint256 x, uint256 i) external pure returns (uint256 r) { assembly { r := byte(i, x) } }
    function signextendOp(uint256 x, uint256 b) external pure returns (uint256 r) { assembly { r := signextend(b, x) } }
    function sarOp(uint256 x, uint256 s) external pure returns (uint256 r) { assembly { r := sar(s, x) } }
    function sdivOp(uint256 x, uint256 y) external pure returns (uint256 r) { assembly { r := sdiv(x, y) } }
    function smodOp(uint256 x, uint256 y) external pure returns (uint256 r) { assembly { r := smod(x, y) } }
    function sltOp(uint256 x, uint256 y) external pure returns (uint256 r, uint256 q) { assembly { r := slt(x, y) q := sgt(x, y) } }
    function expOp(uint256 x, uint256 y) external pure returns (uint256 r) { assembly { r := exp(x, y) } }
    function addmodOp(uint256 x, uint256 y, uint256 m) external pure returns (uint256 r, uint256 q) { assembly { r := addmod(x, y, m) q := mulmod(x, y, m) } }
    function divZero(uint256 x) external pure returns (uint256 r, uint256 q, uint256 p) { assembly { r := div(x, 0) q := mod(x, 0) p := sdiv(x, 0) } }
    function shifts(uint256 x, uint256 s) external pure returns (uint256 a, uint256 b) { assembly { a := shl(s, x) b := shr(s, x) } }
    function notOp(uint256 x) external pure returns (uint256 r, uint256 q) { assembly { r := not(x) q := iszero(x) } }
    function memOps(uint256 x) external pure returns (uint256 r, uint256 q) { assembly { let p := mload(0x40) mstore(p, x) mstore8(add(p, 31), 0xaa) r := mload(p) q := byte(31, mload(p)) } }
    function mcopyOp(bytes calldata d) external pure returns (bytes memory out) { out = new bytes(d.length); assembly { calldatacopy(add(out, 32), d.offset, d.length) let t := mload(0x40) mcopy(t, add(out, 32), d.length) mcopy(add(out, 32), t, d.length) } }
    function mcopyOverlap(uint256 x) external pure returns (uint256 a, uint256 b) { assembly { let p := mload(0x40) mstore(p, x) mstore(add(p, 32), not(x)) mcopy(add(p, 16), p, 32) a := mload(p) b := mload(add(p, 32)) } }
    function calldataOps(uint256 x) external pure returns (uint256 a, uint256 b, uint256 c) { assembly { a := calldatasize() b := calldataload(4) c := calldataload(36) } }
    function returndataOps() external pure returns (uint256 a) { assembly { a := returndatasize() } }
    function keccakOp(uint256 x) external pure returns (bytes32 r, bytes32 q) { assembly { mstore(0, x) r := keccak256(0, 32) q := keccak256(0, 0) } }
    function pushLit() external pure returns (uint256 a, uint256 b, uint256 c) { assembly { a := 0x1234 b := "abc" c := 12345678901234567890 } }
    function pushHexStr() external pure returns (bytes32 a) { assembly { a := "hello world this is exactly 32b!" } }
    function condLogic(uint256 x, uint256 y) external pure returns (uint256 r) { assembly { r := and(or(lt(x, y), eq(x, y)), iszero(gt(x, 100))) } }
    function nestedIf(uint256 x) external pure returns (uint256 r) { assembly { if gt(x, 10) { if lt(x, 20) { r := 1 } if iszero(lt(x, 20)) { r := 2 } } if iszero(gt(x, 10)) { r := 3 } } }
    function solVarInYul(uint8 a, int8 b, bool c, bytes4 d) external pure returns (uint256 x, uint256 y, uint256 z, uint256 w) { assembly { x := a y := b z := c w := d } }
    function yulAssignSol(uint256 raw) external pure returns (uint8 a, int8 b, bool c, bytes4 d) { assembly { a := raw b := raw c := raw d := raw } }
    function yulAssignThenUse(uint256 raw) external pure returns (uint256, int256, bool, bytes32) { uint8 a; int8 b; bool c; bytes4 d; assembly { a := raw b := raw c := raw d := raw } return (a, b, c, d); }
    function yulAssignCmp(uint256 raw) external pure returns (bool, bool, bool, bool) { uint8 a; int8 b; bool c; bytes4 d; assembly { a := raw b := raw c := raw d := raw } return (a == 1, b == -1, c == true, d == 0x00000001); }
    function yulAssignArith(uint256 raw) external pure returns (uint256, int256, uint256) { uint8 a; int8 b; assembly { a := raw b := raw } return (uint256(a) + 1, int256(b) * 2, a * 2); }
    function yulAssignShift(uint256 raw, uint256 s) external pure returns (uint256, uint256) { uint8 a; assembly { a := raw } return (a >> s, a << s); }
    function yulReadAfterSol(uint8 a) external pure returns (uint256 r) { a = a + 1; assembly { r := a } }
    function yulReadAfterSolI(int8 a) external pure returns (uint256 r) { a = -a; assembly { r := a } }
    function yulReadAfterMul(uint8 a, uint8 b) external pure returns (uint256 r) { unchecked { a = a * b; } assembly { r := a } }
    function yulReadAfterShl(uint8 a) external pure returns (uint256 r) { unchecked { a = a << 4; } assembly { r := a } }
    function yulReadAfterNot(uint8 a) external pure returns (uint256 r) { a = ~a; assembly { r := a } }
    function yulReadAfterDiv(int8 a) external pure returns (uint256 r) { a = a / 2; assembly { r := a } }
    function yulReadAfterCast(uint256 a) external pure returns (uint256 r) { uint8 b = uint8(a); assembly { r := b } }
    function yulReadAfterCastI(uint256 a) external pure returns (uint256 r) { int8 b = int8(uint8(a)); assembly { r := b } }
    function yulReadAfterB(bytes32 a) external pure returns (uint256 r) { bytes4 b = bytes4(a); assembly { r := b } }
    function yulReadBool(uint256 a) external pure returns (uint256 r) { bool b = a > 5; assembly { r := b } }
    function yulReadAddr(uint256 a) external pure returns (uint256 r) { address b = address(uint160(a)); assembly { r := b } }
    function yulReadEnum(uint256 a) external pure returns (uint256 r) { E e = E(a); assembly { r := e } }
    enum E { A, B, C }
    function yulReadMemPtr(uint256 a) external pure returns (uint256 r, uint256 q) { uint256[] memory m = new uint256[](2); m[1] = a; assembly { r := mload(add(m, 0x40)) q := mload(m) } }
    function yulReadCdPtr(uint256[] calldata m) external pure returns (uint256 r, uint256 q) { assembly { r := m.offset q := m.length } r -= 4; }
    function yulReadStorageSlot(uint256 v) external returns (uint256 r, uint256 q) { s = v; assembly { r := sload(s.slot) q := s.offset } }
    uint256 s; uint8 t8; uint16 t16;
    function yulReadPacked(uint8 a, uint16 b) external returns (uint256 slot, uint256 o8, uint256 o16, uint256 raw) { t8 = a; t16 = b; assembly { slot := t8.slot o8 := t8.offset o16 := t16.offset raw := sload(t8.slot) } }
    function yulWriteStorage(uint256 v) external returns (uint256) { assembly { sstore(s.slot, v) } return s; }
    function yulTstore(uint256 v) external returns (uint256 r) { assembly { tstore(5, v) r := tload(5) } }
    function yulFnPtr(uint256 a) external pure returns (uint256 r) { function(uint256) internal pure returns (uint256) f = _dbl; assembly { r := f } r = f(a) + (r > 0 ? 0 : 0); }
    function _dbl(uint256 x) internal pure returns (uint256) { return x * 2; }
    function memorySafe(uint256 x) external pure returns (uint256 r) { assembly ("memory-safe") { let p := mload(0x40) mstore(p, x) mstore(0x40, add(p, 32)) r := mload(p) } uint256[] memory m = new uint256[](1); m[0] = r; return m[0]; }
    function memorySafeFmp(uint256 x) external pure returns (uint256 r) { uint256 before; uint256 after_; assembly { before := mload(0x40) } uint256[] memory m = new uint256[](x % 4); assembly { after_ := mload(0x40) } r = after_ - before + m.length; }
    function scratchUse(uint256 x) external pure returns (uint256 r) { assembly { mstore(0, x) mstore(32, not(x)) r := keccak256(0, 64) } bytes32 h = keccak256(abi.encode(x, ~x)); r ^= uint256(h); }
    function zeroSlotUse(uint256 x) external pure returns (uint256 r) { assembly { mstore(0x60, x) } uint256[] memory m = new uint256[](0); assembly { mstore(0x60, 0) } r = m.length + x; }
    function returnAsm(uint256 x) external pure returns (uint256) { assembly { mstore(0, x) return(0, 32) } }
    function revertAsm(uint256 x) external pure returns (uint256) { assembly { mstore(0, x) revert(0, 32) } }
    function stopAsm() external pure { assembly { stop() } }
    function invalidAsm(bool c) external pure returns (uint256) { if (c) assembly { invalid() } return 1; }
    function popCall(uint256 x) external pure returns (uint256 r) { assembly { pop(add(x, 1)) r := x } }
    function clzOp(uint256 x) external pure returns (uint256 r) { assembly { r := clz(x) } }
    function chainId() external view returns (uint256 r, uint256 g) { assembly { r := chainid() g := gasprice() } }
    function verbatimLike(uint256 x) external pure returns (uint256 r) { assembly { r := add(x, 0) r := mul(r, 1) r := or(r, 0) r := xor(r, 0) r := shl(0, r) } }
    function yulStringLit() external pure returns (bytes32 a, bytes32 b) { assembly { a := "" b := "\x01\x02" } }
    function yulNegLit() external pure returns (uint256 a, int256 b) { assembly { a := 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff b := sub(0, 5) } }
    function yulCmpChain(uint256 x) external pure returns (uint256 r) { assembly { r := eq(eq(lt(x, 5), 1), iszero(iszero(gt(x, 1)))) } }
    function yulStackDeep(uint256 x) external pure returns (uint256 r) { assembly { let a := x let b := add(a, 1) let c := add(b, 1) let d := add(c, 1) let e := add(d, 1) let f := add(e, 1) let g := add(f, 1) let h := add(g, 1) let i := add(h, 1) let j := add(i, 1) let k := add(j, 1) let l := add(k, 1) let m := add(l, 1) let n := add(m, 1) let o := add(n, 1) let p := add(o, 1) let q := add(p, 1) r := add(add(add(add(a, b), add(c, d)), add(add(e, f), add(g, h))), add(add(add(i, j), add(k, l)), add(add(m, n), add(add(o, p), q)))) } }
}
