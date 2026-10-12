// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/498_msg_gas_deprecated.sol

contract C {
    function f() public view returns (uint256 val) { return msg.gas; } //~ ERROR: `msg.gas` has been removed
}
