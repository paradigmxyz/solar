// A `using`-attached library function referenced as a bare expression
// statement without being called. solc compiles the statement to a no-op and
// `f` returns normally; solar executes INVALID (0xFE).
// Source: testdata/solidity/test/libsolidity/syntaxTests/using/library_function_attached_but_not_called.sol
library D {
    function double(uint256 self) public pure returns (uint256) {
        return 2 * self;
    }
}

contract UnusedBoundLibraryFunction {
    using D for uint256;

    function f(uint256 a) external pure {
        a.double;
    }
}
