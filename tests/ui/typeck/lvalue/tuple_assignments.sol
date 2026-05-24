//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/tupleAssignments/assignments_to_tuple_and_non_tuple_expressions_of_tuple_types.sol

contract Test {
    uint256 constant CONST = 1;
    uint256 state;

    function testTupleWithConstant() external {
        uint256 x = state;
        (CONST, state) = (x, x); //~ ERROR: cannot assign to a constant variable
    }

    function testTupleWithLiteral() external {
        uint256 x = state;
        (1, state) = (x, x); //~ ERROR: expression has to be an lvalue
        //~^ ERROR: mismatched types
    }

    function returnsTuple() internal returns (uint256, uint256) {}

    function testParenthesizedCallValues() public {
        (returnsTuple()) = (uint256(1), uint256(1)); //~ ERROR: expression has to be an lvalue
        //~^ ERROR: mismatched number of components
    }

    function testTupleHoleTypeMismatch() external {
        uint256 x = state;
        (x, ) = (true, 1); //~ ERROR: mismatched types
        (, x) = (1, true); //~ ERROR: mismatched types
    }
}
