//@ codegen-matrix: standard
// Inline assembly can retype a pointer, so the parameters and returns of the function it reaches
// hold raw bits, including those of the functions that function forwards to.
//@ run-call: direct => 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: nested => 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff

type Id is bytes4;

contract DirtyFixedBytesRetypedPointer {
    function first(Id a) internal pure returns (Id) {
        return a;
    }

    function outer(Id a) internal pure returns (Id) {
        return inner(a);
    }

    function inner(Id a) internal pure returns (Id) {
        return a;
    }

    function direct() external pure returns (bytes32 result) {
        function(Id) internal pure returns (Id) typed = first;
        function(bytes4) internal pure returns (bytes4) retyped;
        assembly { retyped := typed }
        bytes4 value = dirty();
        bytes4 returned = retyped(value);
        assembly { result := returned }
    }

    function nested() external pure returns (bytes32 result) {
        function(Id) internal pure returns (Id) typed = outer;
        function(bytes4) internal pure returns (bytes4) retyped;
        assembly { retyped := typed }
        bytes4 value = dirty();
        bytes4 returned = retyped(value);
        assembly { result := returned }
    }

    function dirty() internal pure returns (bytes4 value) {
        assembly { value := 0x12345678ffffffffffffffffffffffffffffffffffffffffffffffffffffffff }
    }
}
