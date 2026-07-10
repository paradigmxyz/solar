//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/hex_address.sol

contract HexAddress layout at 0xdCad3a6d3569DF655070DEd06cb7A1b2Ccd1D3AF {}
//~^ ERROR: base slot of storage layout must evaluate to an integer
