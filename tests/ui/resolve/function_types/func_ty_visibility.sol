// ported-from: test/libsolidity/syntaxTests/functionTypes/function_types_internal_visibility_error.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/function_types_variable_visibility.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/private_function_type.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/public_function_type.sol

contract C {
    // This is an error, you should explicitly use
    // `external public` to fix it - `internal public` does not exist.
    function(bytes memory) public a; //~ ERROR: invalid visibility
}

contract D {
    function f() public {
        function(uint) private returns (uint) x; //~ ERROR: invalid visibility
    }
}

contract E {
    function f() public {
        function(uint) public returns (uint) x; //~ ERROR: invalid visibility
    }
}

contract F {
    function(bytes memory) a1;
    function(bytes memory) internal b1;
    function(bytes memory) internal internal b2;
    function(bytes memory) external c1;
    function(bytes memory) external internal c2;
    function(bytes memory) external public c3;
}
