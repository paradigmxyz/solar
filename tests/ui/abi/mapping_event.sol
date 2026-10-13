//@ compile-flags: --emit=abi
// Regression test for https://github.com/paradigmxyz/solar/issues/1841.

contract C {
    event E(mapping(uint => uint)[2]); //~ ERROR: types containing mappings cannot be event parameter types
}
