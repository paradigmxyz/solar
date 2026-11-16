// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/address.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/address_constant.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/hex_address.sol

address constant X = 0xdCad3a6d3569DF655070DEd06cb7A1b2Ccd1D3AF;

contract AddressCast layout at address(0x1234) {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract AddressConstant layout at X {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract HexAddress layout at 0xdCad3a6d3569DF655070DEd06cb7A1b2Ccd1D3AF {} //~ ERROR: base slot of storage layout must evaluate to an integer
