// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/413_address_methods.sol

contract C {
    function f() public {
        address payable addr;
        uint balance = addr.balance;
        (bool callSuc,) = addr.call("");
        (bool delegatecallSuc,) = addr.delegatecall("");
        bool sendRet = addr.send(1); //~ WARN: `send` is deprecated and scheduled for removal
        addr.transfer(1); //~ WARN: `transfer` is deprecated and scheduled for removal
        balance; callSuc; delegatecallSuc; sendRet;
    }
}
