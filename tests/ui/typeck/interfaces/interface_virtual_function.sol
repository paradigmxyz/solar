//@ revisions: default allow
//@[allow] compile-flags: --allow=5815
// ported-from: test/libsolidity/syntaxTests/inheritance/interface_virtual_warning.sol

interface I {
    function foo() virtual external; //~[default] WARN: interface functions are implicitly `virtual`
}

// The warning ends the chain solc walks for a `virtual` function: an interface function that is
// also `private` reports it instead of the `virtual` and `private` error.
interface J {
    function foo() private virtual;
    //~[default]^ WARN: interface functions are implicitly `virtual`
    //~^^ ERROR: functions in interfaces must be declared `external`
}

// Elsewhere `virtual` is not implied, so there is nothing to warn about.
abstract contract C {
    function foo() external virtual;
    function bar() private virtual {} //~ ERROR: `virtual` and `private` cannot be used together
}
