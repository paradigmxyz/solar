// ported-from: test/libsolidity/syntaxTests/functionTypes/payable_internal_function_type.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/payable_internal_function_type_is_not_fatal.sol

contract C {
    function (uint) internal payable returns (uint) x; //~ ERROR: only external function types can be payable
}

contract D {
    function (uint) internal payable returns (uint) x; //~ ERROR: only external function types can be payable

    // solc stops after its fatal 7415 error; we keep type checking.
    function g() public {
        x = g; //~ ERROR: mismatched types
    }
}
