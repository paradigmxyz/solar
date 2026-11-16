//@ compile-flags: -Zprint-natspec
// ported-from: test/libsolidity/natspecJSON/dev_explicit_inherit_complex.sol
//~? ERROR: resolved NatSpec for function `ERC20.transfer`
//~? ERROR: resolved NatSpec for function `ERC21.transfer`

import "./auxiliary/Interfaces.sol" as myInterfaces;

contract Token is myInterfaces.ERC20, myInterfaces.ERC21 {
    /// @inheritdoc myInterfaces.ERC20
    //~^ NOTE: inherits NatSpec from function `ERC20.transfer(address,uint256)`
    function transfer(address too, uint amount)
        override(myInterfaces.ERC20, myInterfaces.ERC21) external returns (bool) {
        //~^^ ERROR: resolved NatSpec for function `Token.transfer`
        return false;
    }
}
