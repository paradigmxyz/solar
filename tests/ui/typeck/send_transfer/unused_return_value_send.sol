// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/346_unused_return_value_send.sol

contract test {
    function f() public {
        payable(address(0x12)).send(1);
        //~^ WARN: `send` is deprecated and scheduled for removal
        //~| WARN: failure condition of `send` ignored
    }
}
