//@ codegen-matrix: standard
//@ run-call: assemblyKeepsWord => 0x1ff, 0x1ff
//@ run-call: copy => 0xff
//@ run-call: assignment => 0xff
//@ run-call: internalArgument => 0xff
//@ run-call: internalReturn => 0xff
//@ run-call: ternary true => 0xff
//@ run-call: ternary false => 7
//@ run-call: tuple => 0xff, 1
//@ run-call: bitwise => 0xff
//@ run-call: signedCopy => -1
//@ run-call: boolTest => true, 1
//@ run-call: boolLogic => true, 1
//@ run-call: addressCopy => 0x0000000000000000000000000000000000000001
//@ run-call: addressCompare => true
//@ run-call: fixedBytesCopy => 0x12345678, 0x1234567800000000000000000000000000000000000000000000000000000000
//@ run-call: userDefinedCopy => 0xff
//@ run-call: loop => 0xff
//@ run-call: modifierArgument => 0xff

// Assembly can leave bits above a variable's type. Inside assembly the variable keeps the word
// that assembly wrote; a Solidity read cleans it once, so copies, calls, returns, joins and
// operators carry the clean value, and assembly that reads such a copy sees clean bits.

type Wrapped is uint8;

contract AssemblyBoundaryVariableReads {
    function assemblyKeepsWord() external pure returns (uint256 first, uint256 second) {
        uint8 value;
        assembly {
            value := 0x1ff
            first := value
        }
        assembly {
            second := value
        }
    }

    function copy() external pure returns (uint256 seen) {
        uint8 value;
        assembly { value := 0x1ff }
        uint8 copied = value;
        assembly { seen := copied }
    }

    function assignment() external pure returns (uint256 seen) {
        uint8 value;
        uint8 other;
        assembly { value := 0x1ff }
        other = value;
        assembly { seen := other }
    }

    function internalArgument() external pure returns (uint256) {
        uint8 value;
        assembly { value := 0x1ff }
        return bits(value);
    }

    function internalReturn() external pure returns (uint256 seen) {
        uint8 value = dirty();
        assembly { seen := value }
    }

    function ternary(bool choose) external pure returns (uint256 seen) {
        uint8 value;
        assembly { value := 0x1ff }
        uint8 selected = choose ? value : 7;
        assembly { seen := selected }
    }

    function tuple() external pure returns (uint256 seen, uint256 other) {
        uint8 value;
        assembly { value := 0x1ff }
        (uint8 first, uint8 second) = (value, 1);
        assembly {
            seen := first
            other := second
        }
    }

    function bitwise() external pure returns (uint256 seen) {
        uint8 value;
        assembly { value := 0x1ff }
        uint8 combined = value & 0xff;
        assembly { seen := combined }
    }

    function signedCopy() external pure returns (int256) {
        int8 value;
        assembly { value := 0x1ff }
        int8 copied = value;
        return copied;
    }

    function boolTest() external pure returns (bool isTrue, uint256 seen) {
        bool value;
        assembly { value := 2 }
        bool copied = value;
        isTrue = copied == true;
        assembly { seen := copied }
    }

    function boolLogic() external pure returns (bool isTrue, uint256 seen) {
        bool value;
        assembly { value := 2 }
        bool joined = value && true;
        isTrue = joined;
        assembly { seen := joined }
    }

    function addressCopy() external pure returns (address) {
        address value;
        assembly { value := or(1, shl(160, 1)) }
        address copied = value;
        return copied;
    }

    function addressCompare() external pure returns (bool) {
        address value;
        assembly { value := or(1, shl(160, 1)) }
        return value == address(1);
    }

    function fixedBytesCopy() external pure returns (bytes4 copied, uint256 seen) {
        bytes4 value;
        assembly { value := 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff }
        copied = value;
        assembly { seen := copied }
    }

    function userDefinedCopy() external pure returns (uint256 seen) {
        Wrapped value;
        assembly { value := 0x1ff }
        Wrapped copied = value;
        assembly { seen := copied }
    }

    function loop() external pure returns (uint256 seen) {
        uint8 value;
        assembly { value := 0x1ff }
        for (uint256 i; i < 2; ++i) {
            uint8 copied = value;
            value = copied;
        }
        assembly { seen := value }
    }

    modifier record(uint8 value) {
        uint256 seen;
        assembly { seen := value }
        require(seen == 0xff);
        _;
    }

    function modifierArgument() external pure record(dirty()) returns (uint256) {
        return 0xff;
    }

    function bits(uint8 value) internal pure returns (uint256 seen) {
        assembly { seen := value }
    }

    function dirty() internal pure returns (uint8 value) {
        assembly { value := 0x1ff }
    }
}
