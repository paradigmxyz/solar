//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: arithmetic => 1, 1, 1, 1, 1, 0
//@ run-call: comparisons => true, false, true, false, true, false
//@ run-call: increments => 1, 0
//@ run-call-fail: decrements => Panic(0x11)
//@ run-call-fail: negation => Panic(0x11)
//@ run-call: wideningConversions => 0x78, 0x78
//@ run-call: explicitWideningReturn => 0x78
//@ run-call: implicitReturn => 0x78
//@ run-call-fail: invalidEnum => Panic(0x21)
//@ run-call: assemblyRead => 0x0101
//@ run-call: internalArguments => 0x42, 0x42
//@ run-call: storageAssignment => 1
//@ run-call: signedAssignment => -1
//@ run-call: signedMerge true => -1
//@ run-call: signedMerge false => -2
//@ run-call: signedLoop 0 => -1
//@ run-call: signedLoop 2 => -3
//@ run-call: signedUserAssignment => -1
//@ run-call: signedModifier => -1
//@ run-call: dirtySigned => 0x1234
//@ run-call: signedTry true => -2
//@ run-call: signedTry false => -1
//@ run-call: signedCallTuple true => -2
//@ run-call: signedCallTuple false => -1
//@ run-call: signedAssemblyComparison => 1
//@ run-call: signedTernary true => -1, true
//@ run-call: signedTernary false => 0x1234, false
//@ run-call: signedTupleTernary true => -2, 1
//@ run-call: signedTupleTernary false => 0x1234, 2
//@ run-call: signedUserTernary true => -1
//@ run-call: signedUserTernary false => 0x1234
// ported-from: test/libsolidity/semanticTests/viaYul/cleanup/checked_arithmetic.sol
// ported-from: test/libsolidity/semanticTests/viaYul/cleanup/comparison.sol
// ported-from: test/libsolidity/semanticTests/viaYul/conversion/implicit_cast_assignment.sol
// ported-from: test/libsolidity/semanticTests/operators/userDefined/operator_parameter_cleanup.sol
// ported-from: test/libsolidity/semanticTests/variables/storing_invalid_boolean.sol

type DirtyI8 is int8;
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

    function signedAssignment() external pure returns (int256 raw) {
        int8 value;
        value = -1;
        assembly { raw := value }
    }

    function signedMerge(bool choose) external pure returns (int256 raw) {
        int8 value = -2;
        if (choose) value = -1;
        assembly { raw := value }
    }

    function signedLoop(uint256 count) external pure returns (int256 raw) {
        int8 value;
        value = -1;
        for (uint256 i; i < count; ++i) --value;
        assembly { raw := value }
    }

    function signedUserAssignment() external pure returns (int256 raw) {
        DirtyI8 value;
        value = DirtyI8.wrap(-1);
        assembly { raw := value }
    }

    modifier signedArgument(int8 value) {
        int256 raw;
        assembly { raw := value }
        require(raw == -1);
        _;
    }

    function signedModifier() external pure signedArgument(-1) returns (int256) {
        return -1;
    }

    function signedSource() external pure returns (int8, bytes memory) {
        return (-1, "");
    }

    function signedTry(bool choose) external view returns (int256 raw) {
        try this.signedSource() returns (int8 value, bytes memory) {
            if (choose) value = -2;
            assembly { raw := value }
        } catch {}
    }

    function signedCallTuple(bool choose) external view returns (int256 raw) {
        (int8 value,) = this.signedSource();
        if (choose) value = -2;
        assembly { raw := value }
    }

    function signedAssemblyComparison() external pure returns (uint256 raw) {
        int8 value;
        assembly {
            value := eq(1, 1)
            raw := value
        }
    }

    function signedTernary(bool choose) external pure returns (int256 raw, bool negative) {
        int8 dirty;
        assembly { dirty := 0x1234 }
        int8 selected = choose ? int8(-1) : dirty;
        assembly {
            raw := selected
            negative := slt(selected, 0)
        }
    }

    function signedTupleTernary(bool choose) external pure returns (int256 raw, uint256 other) {
        int8 dirty;
        assembly { dirty := 0x1234 }
        (int8 selected, uint256 companion) =
            choose ? (int8(-2), uint256(1)) : (dirty, uint256(2));
        assembly { raw := selected }
        other = companion;
    }

    function signedUserTernary(bool choose) external pure returns (int256 raw) {
        DirtyI8 dirty;
        assembly { dirty := 0x1234 }
        DirtyI8 selected = choose ? DirtyI8.wrap(-1) : dirty;
        assembly { raw := selected }
    }

    function dirtySigned() external pure returns (uint256 raw) {
        int8 value;
        assembly { value := 0x1234 }
        int8 copied = value;
        if (raw == 0) value = copied;
        assembly { raw := value }
    }

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
