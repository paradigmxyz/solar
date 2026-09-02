// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/355_payable_external.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/356_payable_internal.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/357_payable_private.sol

contract C {
    function f() payable internal {} //~ ERROR: `internal` and `private` functions cannot be payable
    function g() payable private {} //~ ERROR: `internal` and `private` functions cannot be payable

    function h() payable external {}
    function i() payable public {}

    // Only ordinary functions are checked; these are always externally callable.
    constructor() payable {}
    fallback() external payable {}
    receive() external payable {}
}
