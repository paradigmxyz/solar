// ported-from: test/libsolidity/syntaxTests/functionTypes/function_types_internal_visibility_error.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/payable_internal_function_type.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/private_function_type.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/public_function_type.sol

contract C {
    // This is an error, you should explicitly use
    // `external public` to fix it - `internal public` does not exist.
    function(bytes memory) public a; //~ ERROR: invalid visibility

    function (uint) internal payable returns (uint) x; //~ ERROR: only external function types can be payable

    function f() public {
        function(uint) private returns (uint) x; //~ ERROR: invalid visibility
    }

    function g() public {
        function(uint) public returns (uint) x; //~ ERROR: invalid visibility
    }
}

contract D {
    function() external payable a;
    function() external public b;
    function() internal c;
    function() d;

    function() payable e; //~ ERROR: only external function types can be payable
    function() private payable h; //~ ERROR: invalid visibility

    function(function() public) internal i; //~ ERROR: invalid visibility
    function() returns (function() payable) j; //~ ERROR: only external function types can be payable
    mapping(uint => function() private) k; //~ ERROR: invalid visibility

    struct S {
        function() public f; //~ ERROR: invalid visibility
    }

    function l(function() public x) internal {} //~ ERROR: invalid visibility
}
