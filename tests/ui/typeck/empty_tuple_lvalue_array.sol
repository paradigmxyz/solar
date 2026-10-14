// ported-from: test/libsolidity/syntaxTests/types/empty_tuple_lvalue_array.sol

contract C {
    function f() private pure {}
    function a() public {
        uint x;
        uint y;
        (x, y) = [f(), f()];
        //~^ ERROR: array component cannot be empty
        //~| ERROR: array component cannot be empty
    }
}
