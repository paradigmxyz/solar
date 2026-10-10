// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/225_inheriting_from_library.sol

library Lib {}
contract Test is Lib {} //~ ERROR: libraries cannot be inherited from
