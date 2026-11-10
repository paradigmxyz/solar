//@ codegen-matrix: standard
//@ run-call: narrowResult 0x1ff => 0xff
//@ run-call: narrowArgument 0x1ff => 0xff
//@ run-call: boolResult 2 => true, 1

// A call through a function pointer that assembly retyped gets a clean value where it receives
// the callee's result and the callee gets a clean argument, even when the types differ in
// width.
contract AssemblyBoundaryRetypedPointer {
    function narrowResult(uint256 word) external pure returns (uint256) {
        uint8 result = retypeToNarrowResult(passThrough)(word);
        return result;
    }

    function narrowArgument(uint256 word) external pure returns (uint256) {
        return retypeToNarrowArgument(identity8)(word);
    }

    function boolResult(uint256 word) external pure returns (bool isTrue, uint256 seen) {
        bool result = retypeToBoolResult(passThrough)(word);
        isTrue = result;
        assembly { seen := result }
    }

    function passThrough(uint256 word) internal pure returns (uint256) {
        return word;
    }

    function identity8(uint8 value) internal pure returns (uint256 seen) {
        assembly { seen := value }
    }

    function retypeToNarrowResult(function(uint256) internal pure returns (uint256) inFn)
        internal
        pure
        returns (function(uint256) internal pure returns (uint8) outFn)
    {
        assembly { outFn := inFn }
    }

    function retypeToNarrowArgument(function(uint8) internal pure returns (uint256) inFn)
        internal
        pure
        returns (function(uint256) internal pure returns (uint256) outFn)
    {
        assembly { outFn := inFn }
    }

    function retypeToBoolResult(function(uint256) internal pure returns (uint256) inFn)
        internal
        pure
        returns (function(uint256) internal pure returns (bool) outFn)
    {
        assembly { outFn := inFn }
    }
}
