//@ codegen-matrix: standard
//@ run-call: assemblyOnly => 7, 7
//@ run-call-fail: readInvalid => Panic(0x21)
//@ run-call-fail: copyInvalid => Panic(0x21)
//@ run-call-fail: readHighBits => Panic(0x21)
//@ run-call-fail: returnInvalid => Panic(0x21)
//@ run-call: readValid => 1

// An enum that assembly leaves out of range is checked when Solidity reads it. Assembly alone
// sees the word it wrote.
contract AssemblyBoundaryEnumChecks {
    enum Choice {
        Zero,
        One
    }

    function assemblyOnly() external pure returns (uint256 first, uint256 second) {
        Choice choice;
        assembly {
            choice := 7
            first := choice
        }
        assembly { second := choice }
    }

    function readInvalid() external pure returns (uint256) {
        Choice choice;
        assembly { choice := 2 }
        return uint256(choice);
    }

    function copyInvalid() external pure returns (uint256 seen) {
        Choice choice;
        assembly { choice := 2 }
        Choice copied = choice;
        assembly { seen := copied }
    }

    function readHighBits() external pure returns (uint256) {
        Choice choice;
        assembly { choice := 0x101 }
        return uint256(choice);
    }

    function returnInvalid() external pure returns (Choice) {
        return invalid();
    }

    function readValid() external pure returns (uint256) {
        Choice choice;
        assembly { choice := 1 }
        return uint256(choice);
    }

    function invalid() internal pure returns (Choice choice) {
        assembly { choice := 2 }
    }
}
