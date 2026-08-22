//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: wrapBytes1() => 0x01
//@[none, gas, size] run-call: wrapBytes2() => 0x0102
//@[none, gas, size] run-call: wrapBytes4() => 0x01020304
//@[none, gas, size] run-call: wrapBytes32() => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
//@[none, gas, size] run-call: overloadedOr() => 0x03
//@[none, gas, size] run-call: literalReturn() => 0x01
//@[none, gas, size] run-call: widen() => 0x0100
//@[none, gas, size] run-call: roundTrip(bytes1) 0xab => 0xab
//@[none, gas, size] run-call: encodeSelector(bytes4) 0x12345678 => 0x12345678
//@[none, gas, size] run-call: encodeLocalSelector() => 0x12345678

type Byte is bytes1;
type TwoBytes is bytes2;
type FourBytes is bytes4;
type Word is bytes32;

using {orByte as |} for Byte global;

function orByte(Byte lhs, Byte rhs) pure returns (Byte) {
    return Byte.wrap(Byte.unwrap(lhs) | Byte.unwrap(rhs));
}

contract FixedBytesNumericLiterals {
    function wrapBytes1() external pure returns (bytes1) {
        return Byte.unwrap(Byte.wrap(0x01));
    }

    function wrapBytes2() external pure returns (bytes2) {
        return TwoBytes.unwrap(TwoBytes.wrap(0x0102));
    }

    function wrapBytes4() external pure returns (bytes4) {
        return FourBytes.unwrap(FourBytes.wrap(0x01020304));
    }

    function wrapBytes32() external pure returns (bytes32) {
        return Word.unwrap(
            Word.wrap(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)
        );
    }

    function overloadedOr() external pure returns (bytes1) {
        return Byte.unwrap(Byte.wrap(0x01) | Byte.wrap(0x02));
    }

    function literalReturn() external pure returns (bytes1) {
        return 0x01;
    }

    function widen() external pure returns (bytes2) {
        return bytes2(bytes1(0x01));
    }

    function roundTrip(bytes1 value) external pure returns (bytes1) {
        return Byte.unwrap(Byte.wrap(value));
    }

    function encodeSelector(bytes4 selector) external pure returns (bytes4) {
        return bytes4(abi.encodeWithSelector(selector, uint256(1)));
    }

    function encodeLocalSelector() external pure returns (bytes4) {
        bytes4 selector = 0x12345678;
        return bytes4(abi.encodeWithSelector(selector, "abc"));
    }
}
