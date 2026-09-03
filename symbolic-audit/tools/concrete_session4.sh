#!/bin/bash
cd /home/doni/github/paradigmxyz/solar.3
MAX=115792089237316195423570985008687907853269984665640564039457584007913129639935
HALF=57896044618658097711785492504343953926634992332820282019728792003956564819968
MIN=-57896044618658097711785492504343953926634992332820282019728792003956564819968
IMAX=57896044618658097711785492504343953926634992332820282019728792003956564819967
run() { python3 target/symaudit/concrete.py $OPTFLAGS "$@" 2>&1 | grep -v "Revert$\|OutOfGas$\|InvalidFEOpcode$" | head -8; }
F=target/symaudit/np/signed_arith.sol; C=SignedArith
run $F $C 'mul256(int256,int256)' "$MIN -1" "-1 $MIN" "$MIN 2" "$IMAX 2" "$IMAX -1" "-1 -1" "170141183460469231731687303715884105728 -340282366920938463463374607431768211456" "-340282366920938463463374607431768211456 -340282366920938463463374607431768211456"
run $F $C 'mul128(int128,int128)' "-170141183460469231731687303715884105728 -1" "170141183460469231731687303715884105727 2" "-9223372036854775808 -9223372036854775808"
run $F $C 'mul8(int8,int8)' "-128 -1" "127 2" "-64 2" "-64 -2" "11 12" "-12 -11" "127 -1"
run $F $C 'mul8U(int8,int8)' "-128 -1" "127 2" "-64 -2" "127 127"
run $F $C 'div8U(int8,int8)' "-128 -1" "-128 0" "-7 2" "7 -2"
run $F $C 'pow8(int8,uint8)' "-128 1" "-2 7" "-2 8" "2 7" "-3 4" "-3 5" "-1 255" "0 0" "-5 3" "-6 3" "127 2" "-128 2" "-128 0"
run $F $C 'powNeg1(uint256)' 0 1 2 255 256 "$MAX"
run $F $C 'powNeg2(uint256)' 0 1 254 255 256
run $F $C 'powMin(uint256)' 0 1 2
run $F $C 'powBase2(uint256)' 0 6 7 8
run $F $C 'powS(int256,uint256)' "-2 255" "-2 256" "2 255" "$MIN 1" "$MIN 2" "-3 161" "-3 162" "-1 $MAX" "-38685626227668133590597632 8" "-38685626227668133590597631 8" "-16 64" "-16 63"
run $F $C 'powSU(int256,uint256)' "-2 256" "-2 255" "$MIN 2"
run $F $C 'powLit(int256)' "$MIN" "$IMAX" "-340282366920938463463374607431768211456" "340282366920938463463374607431768211455" "340282366920938463463374607431768211456" "-1"
run $F $C 'powLit3(int256)' "$MIN" "-48740834812604276470692694885616" "48740834812604276470692694885616" "-48740834812604276470692694885617" "-1"
run $F $C 'powLit3_8(int8)' "-128" "-5" "-6" "5" "6" "-1"
run $F $C 'mulDivRound(int256)' "$MIN" "$IMAX" "-7" "7" "-1"
run $F $C 'divMinNeg1(int256,int256)' "$MIN -1" "$MIN 0" "7 -2" "-7 2"
run $F $C 'neg(int256)' "$MIN" "$IMAX" 0
run $F $C 'neg8(int8)' -128 127 0
run $F $C 'shr8(int8,uint8)' "-128 7" "-128 8" "-1 255" "127 3"
run $F $C 'shl8(int8,uint8)' "-128 1" "1 7" "1 6" "-1 8" "64 1"
run $F $C 'incMax(int8)' 127 -128 0
run $F $C 'decMin(int8)' -128 127
F=target/symaudit/np2/errors_require.sol; C=ErrorsRequire
run $F $C 'panicExp(uint256,uint256)' "2 255" "2 256" "0 0" "$MAX 1" "$MAX 2" "3 161" "3 162" "10 77" "10 78" "256 32" "256 31" "340282366920938463463374607431768211456 2" "340282366920938463463374607431768211455 2" "7 91" "7 92"
run $F $C 'panicExpNarrow(uint8,uint8)' "2 7" "2 8" "3 5" "3 6" "15 2" "16 2" "255 1" "255 2" "0 0" "1 255"
run $F $C 'panicExpU16(uint16,uint16)' "2 15" "2 16" "255 2" "256 2"
run $F $C 'panicExpU64(uint64,uint64)' "2 63" "2 64" "4294967295 2" "4294967296 2"
run $F $C 'panicExpU128(uint128,uint128)' "2 127" "2 128" "18446744073709551615 2" "18446744073709551616 2"
run $F $C 'panicExpLitBase(uint256)' 0 255 256 "$MAX"
run $F $C 'panicExpLitBase3(uint256)' 0 161 162
run $F $C 'panicExpLitBase10(uint256)' 0 77 78
run $F $C 'panicExpLitBase256(uint256)' 0 31 32
run $F $C 'panicExpLitBaseMax(uint256)' 0 1 2
run $F $C 'panicExpLitExp(uint256)' 0 "340282366920938463463374607431768211455" "340282366920938463463374607431768211456"
run $F $C 'panicExpLitExp3(uint256)' 0 "48740834812604276470692694885616" "48740834812604276470692694885617"
run $F $C 'panicExpLitExp255(uint256)' 0 1 2
run $F $C 'panicExpU8Base(uint8,uint256)' "2 7" "2 8" "2 $MAX" "1 $MAX" "0 0" "255 2" "3 5" "3 6"
run $F $C 'panicExpU8Lit(uint256)' 0 7 8 "$MAX"
run $F $C 'panicExpU8Lit3(uint256)' 0 5 6
run $F $C 'panicExpU8Lit16(uint256)' 0 1 2
run $F $C 'panicExpOne(uint256)' 0 "$MAX"
run $F $C 'panicExpZero(uint256)' 0 1
run $F $C 'panicMul(uint256,uint256)' "$MAX 2" "340282366920938463463374607431768211456 340282366920938463463374607431768211456" "340282366920938463463374607431768211455 340282366920938463463374607431768211457"
run $F $C 'panicMulNarrow(uint8,uint8)' "16 16" "15 17" "255 2"
run $F $C 'panicAlloc(uint256)' 0 1 "$MAX" "3618502788666131106986593281521497120414687020801267626233049500247285301247" "3618502788666131106986593281521497120414687020801267626233049500247285301248" 18446744073709551615 18446744073709551616
run $F $C 'requireCustomEval(bool,uint256)' "false 5" "false $MAX" "false $HALF" "true 5"
run $F $C 'assertExpr(uint256)' 0 1 "$MAX" "$IMAX" "$HALF"
F=target/symaudit/np2/control_flow.sol; C=ControlFlow
run $F $C 'chained(uint256)' 0 5 "$MAX" "$IMAX" "$HALF"
run $F $C 'withAfter(uint256)' 0 5 "$IMAX" "$HALF"
run $F $C 'withBefore(uint256)' 0 5 "$IMAX" "$HALF"
run $F $C 'checkedLoopOverflow(uint8)' 0 250 254 255
run $F $C 'withEarlyMulti(bool,uint256)' "true 3" "false 3"
run $F $C 'withTwice(uint256)' 3 "$HALF"
F=target/symaudit/np2/arrays_memory.sol; C=ArraysMemory
run $F $C 'newString(uint256)' 0 31 32 33 99 100
run $F $C 'newBytes(uint256)' 0 1 31 32 33 99
run $F $C 'newNested(uint256,uint256)' "0 0" "3 3" "4 0"
run $F $C 'multiDim(uint256,uint256)' "0 0" "2 1" "3 0"
run $F $C 'bytesStorage(bytes)' 0x 0x01 0x$(printf 'ab%.0s' {1..31}) 0x$(printf 'ab%.0s' {1..32}) 0x$(printf 'ab%.0s' {1..33})
run $F $C 'bytesStoragePop(bytes)' 0x 0x01 0x$(printf 'ab%.0s' {1..32}) 0x$(printf 'ab%.0s' {1..33})
F=target/symaudit/np3/mappings.sol; C=Mappings
run $F $C 'keyBoolDirty(uint256,uint256)' "0 7" "1 7" "2 7" "$MAX 7"
run $F $C 'keyEnumDirty(uint256,uint256)' "0 7" "2 7" "3 7" "0x101 7"
run $F $C 'keyUdvtDirty(uint256,uint256)' "0 7" "4294967296 7" "4294967297 7"
run $F $C 'bytesVal(uint256,bytes)' "1 0x" "2 0x$(printf 'ab%.0s' {1..31})" "2 0x$(printf 'ab%.0s' {1..32})" "3 0x$(printf 'ab%.0s' {1..40})"
run $F $C 'strVal(uint256,string)' "1 a" "1 $(printf 'k%.0s' {1..31})" "1 $(printf 'k%.0s' {1..32})" "1 $(printf 'k%.0s' {1..33})"
F=target/symaudit/np3/yul_advanced.sol; C=YulAdvanced
run $F $C 'invalidAsm(bool)' true false
run $F $C 'memorySafeFmp(uint256)' 0 1 3 7
run $F $C 'yulAssignArith(uint256)' 0 255 256 0x1ff 0x180 0x7f 0x80 "$MAX"
run $F $C 'yulRecur(uint256)' 0 5 9
run $F $C 'clzOp(uint256)' 0 1 "$MAX" 0x8000000000000000000000000000000000000000000000000000000000000000
run $F $C 'yulAssignCmp(uint256)' 0 1 0x101 0x1ff 0x100 "$MAX"
run $F $C 'yulReadAfterMul(uint8,uint8)' "16 16" "255 255" "2 3"
run $F $C 'yulReadAfterShl(uint8)' 0xff 0x0f 1
run $F $C 'yulReadAfterSolI(int8)' -128 -1 5
F=target/symaudit/np3/functions_inherit.sol; C=D
run $F $C 'deepCallChain(uint256)' 0 5 "$MAX"
run $F $C 'manyRetInternal(uint256)' 0 "$MAX" "3216446923258783206210305139130219662590832907378904556651599555775364712217" "3216446923258783206210305139130219662590832907378904556651599555775364712218"
run $F $C 'ovCall(uint256)' 0 "$MAX" "$IMAX" "$HALF"
run $F $C 'callF()' ""
run $F $C 'callH()' ""
F=target/symaudit/np3/checked_ops.sol; C=CheckedOps
run $F $C 'mapCompoundI(int16,int16)' "-32768 -1" "32767 2" "181 181" "182 181" "-182 181"
run $F $C 'mulAssignI(int8,int8)' "-128 -1" "127 2" "-64 2" "11 12" "-12 -11"
run $F $C 'storageCompound(uint8,uint8)' "16 16" "15 17" "255 2"
run $F $C 'uncheckedExp(uint8,uint8)' "2 8" "2 7" "255 255" "3 6"
run $F $C 'uncheckedMulCmp(int8,int8)' "-128 1" "64 2" "-64 2" "16 8" "-16 8" "-1 -128"
run $F $C 'uncheckedIncLoop(uint8)' 0 254 255
run $F $C 'storageInc(uint8)' 255 0 254
run $F $C 'arrCompoundSide(uint8,uint8)' "1 2" "255 1"
F=target/symaudit/np3/abi_misc.sol; C=AbiMisc
run $F $C 'encDyn(uint256,bytes,uint8)' "1 0x 0" "$MAX 0x$(printf 'ab%.0s' {1..33}) 2"
run $F $C 'encLen(uint8)' 0 4 5
run $F $C 'encDirty(uint256)' 0 0x1ff 0x10000ff "$MAX"
run $F $C 'encPackedDirty(uint256)' 0 0x1ff "$MAX"
run $F $C 'encStructDirty(uint256)' 0 0x1ff "$MAX"
F=target/symaudit/np/storage_packing.sol; C=StoragePacking
run $F $C 'fixedB3Arr(bytes3,uint256)' "0x010203 0" "0x010203 10" "0xffffff 5" "0x000001 11"
run $F $C 'dirtySlot(uint256)' 0 "$MAX" 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
run $F $C 'dirtyStructEnc(uint256)' 0 "$MAX" 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
run $F $C 'dirtyU8Arr(uint256,uint256)' "$MAX 0" "$MAX 31" "0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 1"
echo DONE
