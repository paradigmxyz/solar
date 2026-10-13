// ported-from: test/libsolidity/syntaxTests/errors/zero_signature.sol
error buyAndFree22457070633(uint256); //~ ERROR: the selector `0x00000000` is reserved
contract C {
    error buyAndFree22457070633(uint256); //~ ERROR: the selector `0x00000000` is reserved
}
