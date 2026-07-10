// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_multi_layered_fine.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_multi_layered_error.sol

// ==== Valid: multi-layered with proper override specification ====
interface IBase {
    function foo() external view;
}

contract Base1 is IBase { function foo() public virtual view {} }
contract Base2 is IBase { function foo() public virtual view {} }

interface IExt1a is IBase {}
interface IExt1b is IBase {}
interface IExt2a is IBase {}
interface IExt2b is IBase {}

contract Ext1 is IExt1a, IExt1b, Base1 {}
contract Ext2 is IExt2a, IExt2b, Base2 {}

contract GoodImpl is Ext1, Ext2 {
    function foo() public view override(IBase, Base1, Base2) {}
}

// ==== Invalid: missing override specifier in multi-layered ====
contract Bad1 is Ext1, Ext2 {
    function foo() public view {}
    //~^ ERROR: overriding function is missing `override` specifier
    //~| ERROR: overriding function is missing `override` specifier
    //~| ERROR: Function needs to specify overridden contracts
}
