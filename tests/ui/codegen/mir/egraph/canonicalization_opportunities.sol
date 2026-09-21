//@ codegen-matrix: standard
//@ run-call: upperBound 999 => true
//@ run-call: upperBound 1000 => false
//@ run-call: lowerBound 999 => false
//@ run-call: lowerBound 1000 => true
//@ run-call: signedUpper -2 => true
//@ run-call: signedUpper -1 => false
//@ run-call: signedLower 1 => false
//@ run-call: signedLower 2 => true
//@ run-call: complementOrder 1, 2 => false
//@ run-call: complementOrder 2, 1 => true
//@ run-call: signedComplementOrder -2, -1 => false
//@ run-call: signedComplementOrder 0, -1 => true
//@ run-call: invertSelect true, 7, 11 => 11
//@ run-call: invertSelect false, 7, 11 => 7
//@ run-call: selectTrue false, false => true
//@ run-call: selectTrue true, false => false
//@ run-call: selectTrue true, true => true
//@ run-call: selectFalse false, true => false
//@ run-call: selectFalse true, false => false
//@ run-call: selectFalse true, true => true
//@ run-call: bothNonzero 1, 2 => true
//@ run-call: bothNonzero 0, 2 => false
//@ run-call: eitherNonzero 1, 2 => true
//@ run-call: eitherNonzero 0, 2 => true
//@ run-call: eitherNonzero 0, 0 => false
//@ run-call: byteMask 0x1234 => 0x34
//@ run-call: booleanBit true => 1
//@ run-call: booleanBit false => 0
//@ run-call: boolExtend 0 => false
//@ run-call: boolExtend 2 => true
//@ run-call: extendedOrder 127, 128 => true
//@ run-call: extendedOrder 255, 0 => false
//@ run-call: signedExtendedOrder -128, 127 => true
//@ run-call: signedExtendedOrder 127, -128 => false
//@ run-call: narrowAfterNot 0x1234 => 0xcb
//@ run-call: constantSub 3 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc
//@ run-call: wrappedAdd 3 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc
//@ run-call: complementXor 0x12, 0x34 => 0x26
//@ run-call: invertedEqual 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffedcb => true
//@ run-call: invertedEqual 0x1234 => false
//@ run-call: xorEqual 0x100 => true
//@ run-call: xorEqual 0 => false
//@ run-call: highBit 1 => false
//@ run-call: highBit 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: zeroSign -1 => false
//@ run-call: zeroSign 0 => true
//@ run-call: boolOr false, false => false
//@ run-call: boolOr false, true => true
//@ run-call: boolOr true, false => true
//@ run-call: boolOr true, true => true
//@ run-call: boolAnd false, false => false
//@ run-call: boolAnd false, true => false
//@ run-call: boolAnd true, false => false
//@ run-call: boolAnd true, true => true
//@ run-call: zeroSelect true, 0, 1 => true
//@ run-call: zeroSelect false, 0, 1 => false
//@ run-call: compareSelect true, 7, 11, 7 => true
//@ run-call: compareSelect false, 7, 11, 7 => false
//@ run-call: subtractCompare 7, 7 => true
//@ run-call: subtractCompare 7, 8 => false
//@ run-call: subtractCompare 0, 0x8000000000000000000000000000000000000000000000000000000000000000 => true
//@ run-call: nestedMask 0xabcdef => 0x1cdef
//@ run-call: widenedTrue true => true
//@ run-call: widenedTrue false => false
//@ run-call: widenedFalse true => false
//@ run-call: widenedFalse false => true
//@ run-call: selectTag true => true
//@ run-call: selectTag false => false
//@ run-call: selectDistinct 7, 11 => 7
//@ run-call: selectDistinct 7, 7 => 7
//@ run-call: emptyCall 0 => true
//@ run-call: emptyCall 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: emptyHash 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
//@ run-call: emptyLog 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: emptyCopy 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: emptyReturnCopy 0
//@ run-call-fail: emptyRevert 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x
//@ run-call-fail: emptyReturnCopy 1 => 0x

//@ run-call: factoredNot true, 7, 11 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff8
//@ run-call: factoredNot false, 7, 11 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff4
//@ run-call: constantArms true => 8
//@ run-call: constantArms false => 11
//@ run-call: byteNibble 0x12ff => 15
//@ run-call: clearFields 255, 3, 12 => 240
//@ run-call: associateScale 2, 7 => 210
//@ run-call: oppositeShifts 0xffff => 255
//@ run-call: disjointField 0xffff, 0xabcd => 205
//@ run-call: complementConsumer 40 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd7
//@ run-call: exactSigned -24 => -24
//@ run-call: exactSigned -3 => -3
//@ run-call: unsignedProduct 13 => 13
//@ run-call: signDiscard 255 => 0xff00000000000000000000000000000000000000000000000000000000000000
//@ run-call: compareComposition 1, 2 => true
//@ run-call: compareComposition 2, 1 => false
//@ run-call: compareComposition 2, 2 => true
//@ run-call: zeroSubtract 5, 5 => 0
//@ run-call: zeroSubtract 5, 8 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call: overflowingQuotient 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: evenScale 0, 0x8000000000000000000000000000000000000000000000000000000000000000 => true
//@ run-call: zeroDivisor 25 => 0
//@ run-call: selectSigned true, -128, 127 => -128
//@ run-call: selectSigned false, -128, 127 => 127
//@ run-call: selectBool true, true, false => 1
//@ run-call: selectBool false, true, false => 0
//@ run-call: xorIntersection 15, 5 => 5
//@ run-call: neutralLeft 7 => 10
//@ run-call: neutralLeft 0 => 0
//@ run-call: comparisonFlags 1, 1 => 1
//@ run-call: comparisonFlags 1, 2 => 30
//@ run-call: comparisonFlags 2, 1 => 25
//@ run-call: comparisonFlags 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 26
//@ run-call: comparisonFlags 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => 29
//@ run-call: booleanZeroFlags false, false => 7
//@ run-call: booleanZeroFlags false, true => 1
//@ run-call: booleanZeroFlags true, false => 1
//@ run-call: booleanZeroFlags true, true => 4
//@ run-call: isZeroAddress 0x0000000000000000000000000000000000000000 => true
//@ run-call: isZeroAddress 0x0000000000000000000000000000000000000001 => false
//@ run-call: isZeroAddress 0xffffffffffffffffffffffffffffffffffffffff => false
//@ run-call: isZeroSignedAddress 0x0000000000000000000000000000000000000000 => true
//@ run-call: isZeroSignedAddress 0xffffffffffffffffffffffffffffffffffffffff => false
//@ run-call: mixedSigns 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: mixedSigns 1, 2 => false
//@ run-call: unusedCapacity 3, 5 => 99
//@ run-call: unusedCapacity 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 107
//@ run-call: remainingBudget 3, 5 => 85
//@ run-call: reservationSpan 100, 20 => 112
//@ run-call: reservationSpan 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 33
//@ run-call: undoBase 5, 7 => 44
//@ run-call: comparisonComplements 0, 0 => 6
//@ run-call: comparisonComplements 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 6
contract CanonicalizationOpportunities {
    function upperBound(uint x) external pure returns (bool) { return x <= 999; }
    function lowerBound(uint x) external pure returns (bool) { return x >= 1000; }
    function signedUpper(int x) external pure returns (bool) { return x <= -2; }
    function signedLower(int x) external pure returns (bool) { return x >= 2; }
    function complementOrder(uint a, uint b) external pure returns (bool) { return ~a < ~b; }
    function signedComplementOrder(int a, int b) external pure returns (bool) { return ~a < ~b; }
    function invertSelect(bool choice, uint yes, uint no) external pure returns (uint) { return !choice ? yes : no; }
    function selectTrue(bool choice, bool enabled) external pure returns (bool) { return choice ? enabled : true; }
    function selectFalse(bool choice, bool enabled) external pure returns (bool) { return choice ? enabled : false; }
    function bothNonzero(uint a, uint b) external pure returns (bool r) { assembly { r := and(iszero(iszero(a)), iszero(iszero(b))) } }
    function eitherNonzero(uint a, uint b) external pure returns (bool r) { assembly { r := or(iszero(iszero(a)), iszero(iszero(b))) } }
    function byteMask(uint word) external pure returns (uint r) { assembly { r := byte(31, word) } }
    function booleanBit(bool choice) external pure returns (uint r) { assembly { r := and(choice, 1) } }
    function boolExtend(uint x) external pure returns (bool r) { assembly { r := eq(iszero(iszero(x)), 1) } }
    function extendedOrder(uint8 a, uint8 b) external pure returns (bool) { return uint256(a) < uint256(b); }
    function signedExtendedOrder(int8 a, int8 b) external pure returns (bool) { return int256(a) < int256(b); }
    function narrowAfterNot(uint x) external pure returns (uint8) { return uint8(~x); }
    function constantSub(uint x) external pure returns (uint) { unchecked { return x - 7; } }
    function wrappedAdd(uint x) external pure returns (uint) { unchecked { return x + (type(uint).max - 6); } }
    function complementXor(uint x, uint y) external pure returns (uint) { return ~(~x ^ y); }
    function invertedEqual(uint x) external pure returns (bool) { return ~x == 0x1234; }
    function xorEqual(uint x) external pure returns (bool) { return (x ^ 0x1234) == 0x1334; }
    function highBit(uint x) external pure returns (bool) { return (x >> 255) != 0; }
    function zeroSign(int x) external pure returns (bool) { return (x >> 255) == 0; }
    function boolOr(bool a, bool b) external pure returns (bool r) { assembly { r := iszero(and(iszero(a), iszero(b))) } }
    function boolAnd(bool a, bool b) external pure returns (bool r) { assembly { r := iszero(or(iszero(a), iszero(b))) } }
    function zeroSelect(bool c, uint x, uint y) external pure returns (bool) { return (c ? x : y) == 0; }
    function compareSelect(bool c, uint a, uint b, uint x) external pure returns (bool) { return (c ? a : b) == x; }
    function subtractCompare(uint a, uint b) external pure returns (bool) { unchecked { return a-b == b-a; } }
    function nestedMask(uint x) external pure returns (uint) { return (x & 0xffff) | 0x10000; }
    function widenedTrue(bool flag) external pure returns (bool) { return uint256(flag ? 1 : 0) == 1; }
    function widenedFalse(bool flag) external pure returns (bool) { return uint256(flag ? 1 : 0) != 1; }
    function selectTag(bool choice) external pure returns (bool) { return (choice ? 7 : 11) == 7; }
    function selectDistinct(uint a, uint b) external pure returns (uint) { return a != b ? a : b; }
    function emptyCall(uint pointer) external view returns (bool ok) { assembly { ok := staticcall(gas(), 4, pointer, 0, pointer, 0) } }
    function emptyHash(uint pointer) external pure returns (bytes32 r) { assembly { r := keccak256(pointer, 0) } }
    function emptyRevert(uint pointer) external pure { assembly { revert(pointer, 0) } }
    function emptyLog(uint pointer) external { assembly { log0(pointer, 0) } }
    function emptyCopy(uint pointer) external pure { assembly { calldatacopy(pointer, pointer, 0) } }
    function emptyReturnCopy(uint offset) external pure { assembly { returndatacopy(0, offset, 0) } }

    function factoredNot(bool c, uint x, uint y) external pure returns (uint) { return c ? ~x : ~y; }
    function constantArms(bool c) external pure returns (uint) { unchecked { return (c ? 5 : 8) + 3; } }
    function byteNibble(uint x) external pure returns (uint r) { assembly { r := and(byte(31, x), 15) } }
    function clearFields(uint x, uint a, uint b) external pure returns (uint) { return (x & ~a) & ~b; }
    function associateScale(uint x, uint y) external pure returns (uint) { unchecked { return (x * 3) * (y * 5); } }
    function oppositeShifts(uint x) external pure returns (uint r) { assembly { r := and(shr(16, shl(8, x)), 255) } }
    function disjointField(uint x, uint y) external pure returns (uint) { return ((x & 0xff00) | y) & 255; }
    function complementConsumer(uint x) external pure returns (uint) { unchecked { return ~(x + 7) + 7; } }
    function exactSigned(int x) external pure returns (int) { unchecked { return (x * 8) / 8; } }
    function unsignedProduct(uint128 x) external pure returns (uint) { unchecked { return (uint(x) * 3) / 3; } }
    function signDiscard(uint x) external pure returns (uint r) { assembly { r := shl(248, signextend(0, x)) } }
    function compareComposition(uint x, uint y) external pure returns (bool r) { assembly { r := or(lt(x,y), eq(x,y)) } }
    function zeroSubtract(uint x, uint y) external pure returns (uint) { unchecked { return x == y ? 0 : x - y; } }
    function overflowingQuotient(uint x) external pure returns (uint r) { assembly { r := div(div(x, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), 2) } }
    function evenScale(uint x, uint y) external pure returns (bool) { unchecked { return x * 2 == y * 2; } }
    function zeroDivisor(uint x) external pure returns (uint r) { assembly { r := div(div(x, 7), 0) } }
    function selectSigned(bool c, int8 a, int8 b) external pure returns (int) { return c ? int(a) : int(b); }
    function selectBool(bool c, bool a, bool b) external pure returns (uint) { return c ? (a ? 1 : 0) : (b ? 1 : 0); }
    function xorIntersection(uint x, uint y) external pure returns (uint) { return x & (~x ^ y); }
    function neutralLeft(uint m) external pure returns (uint r) { assembly { r := add(addmod(0, 5, m), mulmod(1, 5, m)) } }

    function comparisonFlags(uint x, uint y) external pure returns (uint r) {
        assembly {
            r := or(or(gt(x, y), eq(y, x)), shl(1, xor(gt(x, y), iszero(eq(x, y)))))
            r := or(r, shl(2, and(slt(x, y), iszero(eq(y, x)))))
            r := or(r, shl(3, or(sgt(x, y), iszero(eq(y, x)))))
            r := or(r, shl(4, or(lt(x, y), lt(y, x))))
        }
    }
    function booleanZeroFlags(bool a, bool b) external pure returns (uint r) {
        assembly { r := or(or(iszero(and(a, b)), shl(1, iszero(or(a, b)))), shl(2, iszero(xor(a, b)))) }
    }
    function isZeroAddress(address account) external pure returns (bool) { return uint256(uint160(account)) == 0; }
    function isZeroSignedAddress(address account) external pure returns (bool) { return int256(int160(uint160(account))) == 0; }
    function mixedSigns(uint x, uint y) external pure returns (bool r) { assembly { r := and(lt(x, y), slt(y, x)) } }
    function unusedCapacity(uint x, uint y) external pure returns (uint) { unchecked { return (100 - x) - y + 7; } }
    function remainingBudget(uint x, uint y) external pure returns (uint) { unchecked { return (100 - x) - y - 7; } }
    function reservationSpan(uint end, uint start) external pure returns (uint) { unchecked { return (end - (start + 32)) + 64; } }
    function undoBase(uint end, uint length) external pure returns (uint) { unchecked { return (end - (32 - length)) + 64; } }
    function comparisonComplements(uint x, uint y) external pure returns (uint r) {
        assembly {
            let same := eq(x, y)
            let different := iszero(eq(y, x))
            r := or(or(and(same, different), shl(1, or(same, different))), shl(2, xor(same, different)))
        }
    }
}
