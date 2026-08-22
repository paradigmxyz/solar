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
//@[none] run-call: arithmetic() => 1, 1, 1, 1, 1, 0
//@[gas] run-call: arithmetic() => 1, 1, 1, 1, 1, 0
//@[size] run-call: arithmetic() => 1, 1, 1, 1, 1, 0
//@[none] run-call: comparisons() => true, false, true, false, true, false
//@[gas] run-call: comparisons() => true, false, true, false, true, false
//@[size] run-call: comparisons() => true, false, true, false, true, false
//@[none] run-call: increments() => 1, 0
//@[gas] run-call: increments() => 1, 0
//@[size] run-call: increments() => 1, 0
//@[none] run-call-fail: decrements() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[gas] run-call-fail: decrements() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[size] run-call-fail: decrements() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[none] run-call-fail: negation() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[gas] run-call-fail: negation() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[size] run-call-fail: negation() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[none] run-call: wideningConversions() => 0x78, 0x78
//@[gas] run-call: wideningConversions() => 0x78, 0x78
//@[size] run-call: wideningConversions() => 0x78, 0x78
//@[none] run-call: explicitWideningReturn() => 0x78
//@[gas] run-call: explicitWideningReturn() => 0x78
//@[size] run-call: explicitWideningReturn() => 0x78
//@[none] run-call: implicitReturn() => 0x78
//@[gas] run-call: implicitReturn() => 0x78
//@[size] run-call: implicitReturn() => 0x78
//@[none] run-call-fail: invalidEnum() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@[gas] run-call-fail: invalidEnum() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@[size] run-call-fail: invalidEnum() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@[none] run-call: assemblyRead() => 0x0101
//@[gas] run-call: assemblyRead() => 0x0101
//@[size] run-call: assemblyRead() => 0x0101
//@[none] run-call: internalArguments() => 0x42, 0x42
//@[gas] run-call: internalArguments() => 0x42, 0x42
//@[size] run-call: internalArguments() => 0x42, 0x42
//@[none] run-call: storageAssignment() => 1
//@[gas] run-call: storageAssignment() => 1
//@[size] run-call: storageAssignment() => 1
// ported-from: test/libsolidity/semanticTests/viaYul/cleanup/checked_arithmetic.sol
// ported-from: test/libsolidity/semanticTests/viaYul/cleanup/comparison.sol
// ported-from: test/libsolidity/semanticTests/viaYul/conversion/implicit_cast_assignment.sol
// ported-from: test/libsolidity/semanticTests/operators/userDefined/operator_parameter_cleanup.sol
// ported-from: test/libsolidity/semanticTests/variables/storing_invalid_boolean.sol

type DirtyU8 is uint8;
using {dirtyNot as ~} for DirtyU8 global;

function dirtyNot(DirtyU8 value) pure returns (DirtyU8 result) {
    assembly {
        result := div(value, 256)
    }
}

contract InlineAssemblyScalarCleanup {
    enum Choice {
        Zero,
        One
    }

    bool private stored;

    function arithmetic() external pure returns (uint8, uint8, uint8, uint8, uint8, uint8) {
        uint8 value;
        assembly {
            value := 0x0101
        }
        return (value + 0, value * 1, value / 1, value % 2, value << 0, value >> 1);
    }

    function comparisons() external pure returns (bool, bool, bool, bool, bool, bool) {
        uint8 value;
        assembly {
            value := 0x0101
        }
        return (value == 1, value != 1, value >= 1, value <= 0, value > 0, value < 1);
    }

    function increments() external pure returns (uint8 pre, uint8 post) {
        assembly {
            pre := 0x0100
            post := 0x0100
        }
        return (++pre, post++);
    }

    function decrements() external pure returns (uint8 value) {
        assembly {
            value := not(0xff)
        }
        return --value;
    }

    function negation() external pure returns (int8 value) {
        assembly {
            value := 0x80
        }
        return -value;
    }

    function wideningConversions() external pure returns (uint16 assigned, uint256 called) {
        uint8 value;
        assembly {
            value := 0x12345678
        }
        assigned = value;
        called = widen(value);
    }

    function explicitWideningReturn() external pure returns (uint256) {
        uint8 value;
        assembly {
            value := 0x12345678
        }
        return value;
    }

    function implicitReturn() external pure returns (uint8 value) {
        assembly {
            value := 0x12345678
        }
    }

    function invalidEnum() external pure returns (Choice value) {
        assembly {
            value := 2
        }
        value == Choice.Zero;
    }

    function assemblyRead() external pure returns (uint256 raw) {
        uint8 value;
        assembly {
            value := 0x0101
            raw := value
        }
    }

    function internalArguments() external pure returns (DirtyU8, DirtyU8) {
        DirtyU8 value;
        assembly {
            value := 0x4200
        }
        return (~value, dirtyNot(value));
    }

    function storageAssignment() external returns (uint256 raw) {
        bool value;
        assembly {
            value := 5
        }
        stored = value;
        assembly {
            raw := sload(stored.slot)
        }
    }

    function widen(uint256 value) internal pure returns (uint256) {
        return value;
    }
}
