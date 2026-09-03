//@ codegen-matrix: standard
//@ run-call: multipleReturns => true
//@ run-call: functionPointer => true

contract DirtyInternalMultiReturn {
    function dirtyPair() internal pure returns (uint256, uint8 value) {
        assembly ("memory-safe") {
            value := 0x101
        }
    }

    function dirty(uint8) internal pure returns (uint8 value) {
        assembly ("memory-safe") {
            value := 0x101
        }
    }

    function multipleReturns() external pure returns (bool) {
        (, uint8 value) = dirtyPair();
        return value == 1;
    }

    function functionPointer() external pure returns (bool) {
        function(uint8) internal pure returns (uint8) target = dirty;
        return target(1) == 1;
    }
}
