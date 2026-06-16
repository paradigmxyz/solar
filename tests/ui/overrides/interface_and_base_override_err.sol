// ported-from: test/libsolidity/syntaxTests/inheritance/override/interface_and_base_override_err.sol

interface IBase {
    function foo() external view;
}

contract Base is IBase {
    function foo() public virtual view {}
}

interface IExt is IBase {}

contract Ext is IExt, Base {}

contract Impl is Ext {
    function foo() public view {}
    //~^ ERROR: overriding function is missing `override` specifier
    //~| ERROR: Function needs to specify overridden contracts
}
