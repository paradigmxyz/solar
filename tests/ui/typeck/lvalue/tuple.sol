//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/array/bytes1_array_push_assign_multi.sol
// ported-from: test/libsolidity/syntaxTests/tupleAssignments/assignments_to_tuple_and_non_tuple_expressions_of_tuple_types.sol

contract Test {
    uint256 constant CONST = 1;
    uint256 state;
    bytes1[] byteArray;
    bytes1[] otherByteArray;

    function testTupleWithConstant() external {
        uint256 x = state;
        (CONST, state) = (x, x); //~ ERROR: cannot assign to a constant variable
    }

    function testTupleWithLiteral() external {
        uint256 x = state;
        (1, state) = (x, x); //~ ERROR: expression has to be an lvalue
        //~^ ERROR: mismatched types
    }

    function tuplePushLvalues() external {
        (byteArray.push(), byteArray.push()) = (bytes1(0), bytes1(0));
        (((byteArray.push())), (byteArray.push())) = (bytes1(0), bytes1(0));
        ((byteArray.push(), byteArray.push()), byteArray.push()) =
            ((bytes1(0), bytes1(0)), bytes1(0));
        (byteArray.push(), byteArray[0]) = (bytes1(0), bytes1(0));
        bytes1[] storage local = byteArray;
        (byteArray.push(), local.push()) = (bytes1(0), bytes1(0));
        (byteArray.push(), otherByteArray.push()) = (bytes1(0), bytes1(0));
    }

    function returnsTuple() internal returns (uint256, uint256) {}

    function testParenthesizedCallValues() public {
        (returnsTuple()) = (uint256(1), uint256(1)); //~ ERROR: expression has to be an lvalue
    }
}
