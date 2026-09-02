// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/354_payable_in_library.sol

library L {
    function f() public payable {} //~ ERROR: library functions cannot be payable
    function g() external payable {} //~ ERROR: library functions cannot be payable
    function h() public {}
}

// The restriction only applies to libraries.
contract C {
    function f() public payable {}
}
