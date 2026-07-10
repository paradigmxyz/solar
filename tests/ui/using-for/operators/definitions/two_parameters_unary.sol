//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/operator_taking_two_parameters_unary.sol

type Int is int128;

using {bitnot as ~} for Int global;

function bitnot(Int, Int) pure returns (Int) {}
//~^ ERROR: wrong parameters

contract C {
    function test() public pure {
        ~Int.wrap(1);
        //~^ ERROR: cannot apply unary operator
    }
}
