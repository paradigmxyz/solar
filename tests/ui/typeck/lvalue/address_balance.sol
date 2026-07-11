// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/015_balance_invalid.sol

contract Test {
    function testAddressMember() external {
        address(0).balance = 7; //~ ERROR: expression has to be an lvalue
    }
}
