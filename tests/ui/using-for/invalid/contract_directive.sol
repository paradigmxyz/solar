//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/using_contract_err.sol

contract NotLibrary {
    function f(uint256 x) public pure returns (uint256) {
        return x;
    }
}

contract C {
    using NotLibrary for uint256; //~ ERROR: library name expected
}
