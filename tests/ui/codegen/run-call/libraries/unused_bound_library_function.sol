//@ codegen-matrix: standard
//@ run-call: f 1
// ported-from: test/libsolidity/syntaxTests/using/library_function_attached_but_not_called.sol

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
