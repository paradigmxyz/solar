// ported-from: test/libsolidity/syntaxTests/using/using_functions_with_ast.sol

function f(uint256) pure {}

contract C {
    using {f} for *; //~ ERROR: the type has to be specified explicitly when attaching specific functions
}
