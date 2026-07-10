//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/tupleAssignments/assignments_to_tuple_and_non_tuple_expressions_of_tuple_types.sol

contract C {
    uint256[] public array;

    function f() public {
        (f()) = (); //~ ERROR: expression has to be an lvalue
    }

    function g() public {
        (revert()) = (); //~ ERROR: expression has to be an lvalue
    }

    function h() internal returns (uint256, uint256) {}

    function i() public {
        (h()) = (1, 1); //~ ERROR: expression has to be an lvalue
    }

    function j() public returns (uint256, uint256) {
        (j()) = (1, 1); //~ ERROR: expression has to be an lvalue
    }

    function m() public {
        (uint256 x, uint256 y) = (1, 1);
        x;
        y;
    }

    function n() public {
        ((array.push(), array.push())) = (1, 1);
    }
}
