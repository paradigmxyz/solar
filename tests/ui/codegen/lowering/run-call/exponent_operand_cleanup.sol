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
//@[none, gas, size] run-call: checkedDirtyOperands() => 9, -27
//@[none, gas, size] run-call: uncheckedDirtyOperands() => 4
//@[none, gas, size] run-call: copiedDirtyOperand() => 9
//@[none, gas, size] run-call: branchedDirtyOperand(bool) true => -27
//@[none, gas, size] run-call: branchedDirtyOperand(bool) false => 9
//@[none, gas, size] run-call: fullWidthOperands() => 16
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
}
