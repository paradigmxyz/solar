//@ codegen-matrix: standard
//@ run-call: scale 7 => 105
//@ run-call: scale 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1
//@ run-call: sameFee 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0, 1 => false
//@ run-call: sameFee 9, 9, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: oddScale 7, 7 => true
//@ run-call: oddScale 0, 0x8000000000000000000000000000000000000000000000000000000000000000 => false
//@ run-call: nestedDifference 3, 8 => 8
//@ run-call: nestedDifference 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: invert 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: invert 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: nestedAnd 0x123, 0xff => 0x23
//@ run-call: nestedOr 0x123, 0xff => 0x1ff
//@ run-call: accumulatedFlags 0 => 0x120
//@ run-call: toggleFlags 0x120 => 0x100
//@ run-call: page 1023, 8 => 3
//@ run-call: page 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 255 => 1
//@ run-call: page 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 256 => 0
//@ run-call: page 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: normalized 0 => 0
//@ run-call: normalized 19 => 1
//@ run-call: emptyPower 0 => 1
//@ run-call: emptyPower 256 => 0
//@ run-call: negate -7 => 7
//@ run-call: negate -57896044618658097711785492504343953926634992332820282019728792003956564819968 => -57896044618658097711785492504343953926634992332820282019728792003956564819968
//@ run-call: ringAdd 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 1
//@ run-call: ringMul 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 254
//@ run-call: reduceAdd 511, 256 => 255
//@ run-call: reduceMul 511, 256 => 255
//@ run-call: belowBucket 6, 7 => true
//@ run-call: belowBucket 7, 7 => false
//@ run-call: belowBucket 7, 0 => true
//@ run-call: nestedQuotient 83 => 3
//@ run-call: subset 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 255 => false
//@ run-call: superset 0, 255 => false
//@ run-call: shiftComplement -1, 300 => 0
//@ run-call: shiftComplement 0, 256 => -1
//@ run-call: signedNegative -1 => true
//@ run-call: signedNegative 4095 => false
//@ run-call: highSign -1 => -1
//@ run-call: highSign 1 => 0
//@ run-call: signMask 0x80 => 0x80
//@ run-call: signMask 0x17f => 0x7f
//@ run-call: highByte 0x80 => 255
//@ run-call: highByte 0x7f => 0
//@ run-call: realign 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: clearLow 0x1234 => 0x1200
//@ run-call: affineConstant 24 => true
//@ run-call: affineConstant 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => false
//@ run-call: booleanMask 0x123, 0xff => true
//@ run-call-fail: reduceAdd 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call-fail: reduceMul 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call: complementSar -129, 8 => -1
//@ run-call: complementSar 129, 256 => 0
//@ run-call: sarOnes 0 => -1
//@ run-call: sarOnes 256 => -1
//@ run-call: signedMinQuotient -57896044618658097711785492504343953926634992332820282019728792003956564819968 => 1
//@ run-call: signedMinQuotient -1 => 0
//@ run-call: fixedComplement 0 => false
//@ run-call: shiftedOrder 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 256 => false
//@ run-call: tagDifference 0xdead, 0x55, 0xaa => 0xff
//@ run-call: repeatedPermission 0x123, 0xff, 0xffff => 0x23
//@ run-call: fillOutsideMask 0x12, 0xff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff12
//@ run-call: negatedParity 3 => 1
//@ run-call: signFromBit 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: signFromBit 1 => 0
//@ run-call: shiftedDistance 5, 2, 8 => 768
//@ run-call: shiftedDistance 5, 2, 256 => 0
//@ run-call: extractMasked 0x1234, 0xff00, 8 => 0x34
//@ run-call: extractMasked 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 256 => 0
//@ run-call: signedAligned -256 => true
//@ run-call: signedAligned -255 => false
//@ run-call: removeSetMask 0x1234 => 0x1200
//@ run-call: combinedFee 3, 7, 11 => 54
//@ run-call: signedDelta 3, 8 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb
//@ run-call: belowUnit 999 => true
//@ run-call: belowUnit 1000 => false
//@ run-call: minimumOnly -57896044618658097711785492504343953926634992332820282019728792003956564819968 => true
//@ run-call: minimumOnly 0 => false
//@ run-call: alternatingSign 0 => 1
//@ run-call: alternatingSign 3 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: signedTopByte 0x8000000000000000000000000000000000000000000000000000000000000000 => -128
//@ run-call: signedTopByte 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 127

//@ run-call: firstRingIndex 0 => 0
//@ run-call: firstRingIndex 1 => 0
//@ run-call: firstRingIndex 2 => 1
//@ run-call: firstRingIndex 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 1
contract RuleOpportunities {
    function scale(uint x) external pure returns (uint r) { unchecked { return (x * 3) * 5; } }
    function sameFee(uint x, uint y, uint fee) external pure returns (bool) { unchecked { return x + fee == y + fee; } }
    function oddScale(uint x, uint y) external pure returns (bool) { unchecked { return x * 3 == y * 3; } }
    function nestedDifference(uint balance, uint amount) external pure returns (uint) { unchecked { return balance - (balance - amount); } }
    function invert(uint x) external pure returns (uint r) { assembly { r := sub(not(0), x) } }
    function nestedAnd(uint flags, uint mask) external pure returns (uint) { return flags & (flags & mask); }
    function nestedOr(uint flags, uint mask) external pure returns (uint) { return flags | (flags | mask); }
    function accumulatedFlags(uint flags) external pure returns (uint) { return (flags | 0x100) | 0x20; }
    function toggleFlags(uint flags) external pure returns (uint) { return (flags ^ 0x100) ^ 0x120; }
    function page(uint offset, uint bits) external pure returns (uint r) { assembly { r := div(offset, shl(bits, 1)) } }
    function normalized(uint amount) external pure returns (uint r) { assembly { r := div(amount, amount) } }
    function emptyPower(uint count) external pure returns (uint r) { assembly { r := exp(0, count) } }
    function negate(int value) external pure returns (int r) { assembly { r := sdiv(value, not(0)) } }
    function ringAdd(uint x, uint y) external pure returns (uint) { return addmod(x, y, 256); }
    function ringMul(uint x, uint y) external pure returns (uint) { return mulmod(x, y, 256); }
    function reduceAdd(uint x, uint modulus) external pure returns (uint) { return addmod(x, 0, modulus); }
    function reduceMul(uint x, uint modulus) external pure returns (uint) { return mulmod(x, 1, modulus); }
    function belowBucket(uint amount, uint unit) external pure returns (bool r) { assembly { r := iszero(div(amount, unit)) } }
    function nestedQuotient(uint amount) external pure returns (uint r) { assembly { r := div(div(amount, 7), 3) } }
    function subset(uint flags, uint mask) external pure returns (bool) { return (flags & mask) > flags; }
    function superset(uint flags, uint mask) external pure returns (bool) { return (flags | mask) < flags; }
    function shiftComplement(int value, uint bits) external pure returns (int) { return (~value) >> bits; }
    function signedNegative(int value) external pure returns (bool) { return (value >> 12) < 0; }
    function highSign(int value) external pure returns (int) { return value >> 300; }
    function signMask(uint value) external pure returns (uint r) { assembly { r := and(signextend(0, value), 255) } }
    function highByte(uint value) external pure returns (uint r) { assembly { r := shr(248, signextend(0, value)) } }
    function realign(uint value) external pure returns (uint) { return (value << 8) >> 8; }
    function clearLow(uint value) external pure returns (uint) { return (value >> 8) << 8; }
    function affineConstant(uint x) external pure returns (bool) { unchecked { return x + 7 == 31; } }
    function booleanMask(uint flags, uint mask) external pure returns (bool) { return ((flags & mask) | mask) == mask; }
    function complementSar(int value, uint bits) external pure returns (int) { return ~((~value) >> bits); }
    function sarOnes(uint bits) external pure returns (int) { return int256(-1) >> bits; }
    function signedMinQuotient(int value) external pure returns (int r) { assembly { r := sdiv(value, shl(255, 1)) } }
    function fixedComplement(uint value) external pure returns (bool) { return value == ~value; }
    function shiftedOrder(uint value, uint bits) external pure returns (bool) { return (value >> bits) > value; }
    function tagDifference(uint tag, uint x, uint y) external pure returns (uint) { return (tag ^ x) ^ (tag ^ y); }
    function repeatedPermission(uint flags, uint allowed, uint extra) external pure returns (uint) { return (flags & allowed) & (flags | extra); }
    function fillOutsideMask(uint value, uint mask) external pure returns (uint) { return (value & mask) | ~mask; }
    function negatedParity(uint value) external pure returns (uint) { unchecked { return (0 - value) & 1; } }
    function signFromBit(uint value) external pure returns (uint) { unchecked { return 0 - (value >> 255); } }
    function shiftedDistance(uint x, uint y, uint bits) external pure returns (uint) { unchecked { return (x << bits) - (y << bits); } }
    function extractMasked(uint value, uint mask, uint bits) external pure returns (uint) { return ((value << bits) & mask) >> bits; }
    function signedAligned(int value) external pure returns (bool r) { assembly { r := iszero(smod(value, 256)) } }
    function removeSetMask(uint value) external pure returns (uint) { unchecked { return (value | 255) - 255; } }
    function combinedFee(uint rate, uint a, uint b) external pure returns (uint) { unchecked { return rate * a + rate * b; } }
    function signedDelta(uint a, uint b) external pure returns (uint) { unchecked { return a + (0 - b); } }
    function belowUnit(uint value) external pure returns (bool) { return value / 1000 == 0; }
    function minimumOnly(int value) external pure returns (bool) { return value < type(int256).min + 1; }
    function alternatingSign(uint exponent) external pure returns (uint r) { assembly { r := exp(not(0), exponent) } }
    function signedTopByte(uint value) external pure returns (int r) { assembly { r := signextend(0, shr(248, value)) } }
    function firstRingIndex(uint capacity) external pure returns (uint r) { assembly { r := mod(1, capacity) } }
}
