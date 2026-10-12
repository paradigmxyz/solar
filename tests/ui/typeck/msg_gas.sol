// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/498_msg_gas_deprecated.sol

contract C {
    function f() public view returns (uint256 val) { return msg.gas; } //~ ERROR: `msg.gas` has been removed
}

contract D {
    struct S { uint256 gas; }

    function g() public {
        msg.gas = 1; //~ ERROR: `msg.gas` has been removed
        //~^ ERROR: expression has to be an lvalue
    }

    function h() public pure returns (uint256) {
        S memory msg = S(1);
        return msg.gas;
    }
}
