// ported-from: test/libsolidity/syntaxTests/using/external_function_qualified_with_this.sol

contract C {
    using {this.contractFunction} for uint256; //~ ERROR: `this` is a builtin

    function contractFunction(uint256) external view {}
}
