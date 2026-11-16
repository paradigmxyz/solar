// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/354_payable_in_library.sol

library L {
    function f() public payable {} //~ ERROR: library functions cannot be payable
    function g() external payable {} //~ ERROR: library functions cannot be payable
    function h() public {}

    // solc reports 7708 for these too, not 5587 ("`internal` and `private`
    // functions cannot be payable"), because the library check comes first.
    function i() internal payable {} //~ ERROR: library functions cannot be payable
    function j() private payable {} //~ ERROR: library functions cannot be payable
}

// The restriction only applies to libraries.
contract C {
    function f() public payable {}
}
