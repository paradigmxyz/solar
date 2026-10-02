//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: checkedDirtyOperands => 9, -27
//@ run-call: uncheckedDirtyOperands => 4
//@ run-call: copiedDirtyOperand => 9
//@ run-call: branchedDirtyOperand true => -27
//@ run-call: branchedDirtyOperand false => 9
//@ run-call: fullWidthOperands => 16
//@ run-call: signedProduct -57896044618658097711785492504343953926634992332820282019728792003956564819968, 0 => 0
//@ run-call: signedProduct -57896044618658097711785492504343953926634992332820282019728792003956564819968, 1 => -57896044618658097711785492504343953926634992332820282019728792003956564819968
//@ run-call: signedProduct -7, 9 => -63
//@ run-call: signedPower -57896044618658097711785492504343953926634992332820282019728792003956564819968, 0 => 1
//@ run-call: signedPower -57896044618658097711785492504343953926634992332820282019728792003956564819968, 1 => -57896044618658097711785492504343953926634992332820282019728792003956564819968
//@ run-call: signedPower -3, 3 => -27
//@ run-call-fail: signedProduct -57896044618658097711785492504343953926634992332820282019728792003956564819968, -1 => Panic(0x11)
//@ run-call-fail: signedProduct -57896044618658097711785492504343953926634992332820282019728792003956564819968, -2 => Panic(0x11)
//@ run-call-fail: signedProduct -1, -57896044618658097711785492504343953926634992332820282019728792003956564819968 => Panic(0x11)
//@ run-call-fail: signedProduct 57896044618658097711785492504343953926634992332820282019728792003956564819967, 2 => Panic(0x11)
//@ run-call-fail: signedPower -57896044618658097711785492504343953926634992332820282019728792003956564819968, 2 => Panic(0x11)
//@ run-call-fail: signedPower 57896044618658097711785492504343953926634992332820282019728792003956564819967, 2 => Panic(0x11)
//@ run-call-fail: narrowSignedPower -128, 2 => Panic(0x11)
// ported-from: test/libsolidity/semanticTests/exponentiation/signed_base.sol
// ported-from: test/libsolidity/semanticTests/exponentiation/small_exp.sol

contract ExponentOperandCleanup {
    function checkedDirtyOperands() external pure returns (int256, int256) {
        int32 base = -3;
        uint8 evenExponent;
        uint8 oddExponent;
        assembly {
            evenExponent := 0x102
            oddExponent := 0x103
        }
        return (base**evenExponent, base**oddExponent);
    }

    function uncheckedDirtyOperands() external pure returns (uint256 result) {
        uint32 base;
        uint8 exponent;
        assembly {
            base := 0xfffffffffe
            exponent := 0x102
        }
        unchecked {
            result = base**exponent;
        }
    }

    function copiedDirtyOperand() external pure returns (uint256) {
        uint8 exponent;
        assembly {
            exponent := 0x102
        }
        uint8 copy = exponent;
        return 3**copy;
    }

    function branchedDirtyOperand(bool chooseDirty) external pure returns (int256) {
        uint8 exponent = 2;
        if (chooseDirty) {
            assembly {
                exponent := 0x103
            }
        }
        return int256(-3)**exponent;
    }

    function fullWidthOperands() external pure returns (uint256) {
        return 2**4;
    }

    function signedProduct(int256 a, int256 b) external pure returns (int256) {
        return a * b;
    }

    function signedPower(int256 base, uint256 exponent) external pure returns (int256) {
        return base**exponent;
    }

    function narrowSignedPower(int8 base, uint8 exponent) external pure returns (int8) {
        return base**exponent;
    }
}
