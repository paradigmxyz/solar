// ported-from: test/libsolidity/syntaxTests/functionTypes/function_types_internal_visibility_error.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/payable_internal_function_type.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/private_function_type.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/public_function_type.sol

contract C {
    // This is an error, you should explicitly use
    // `external public` to fix it - `internal public` does not exist.
    function(bytes memory) public a; //~ ERROR: invalid visibility
}

contract D {
    function (uint) internal payable returns (uint) x; //~ ERROR: only external function types can be payable
}

contract E {
    function f() public {
        function(uint) private returns (uint) x; //~ ERROR: invalid visibility
    }
}

contract F {
    function f() public {
        function(uint) public returns (uint) x; //~ ERROR: invalid visibility
    }
}
