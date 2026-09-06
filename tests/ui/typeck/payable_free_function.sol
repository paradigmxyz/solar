// ported-from: test/libsolidity/syntaxTests/freeFunctions/free_payable.sol

function f() payable {} //~ ERROR: free functions cannot be payable

function g() {}
