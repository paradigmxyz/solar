// Tests for complex multi-layered inheritance chains
// Based on solc tests: override_multi_layered_fine.sol, override_multi_layered_error.sol

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
    //~^ ERROR: overriding function is missing "override" specifier
    //~| ERROR: overriding function is missing "override" specifier
    //~| ERROR: Function needs to specify overridden contracts
}

// ==== Invalid: incomplete override list ====
contract Bad2 is Ext1, Ext2 {
    function foo() public view override(Base1) {}
    //~^ ERROR: Function needs to specify overridden contracts
}

// ==== Invalid: missing base in override list ====
contract Bad3 is Ext1, Ext2 {
    function foo() public view override(IBase, Base1) {}
    //~^ ERROR: Function needs to specify overridden contracts
}

// ==== Valid: three-layer deep inheritance ====
interface IDeep {}
contract DeepBase is IDeep {
    function f() external virtual {}
}
contract DeepMid1 is DeepBase {}
contract DeepMid2 is DeepBase {}
contract DeepLeaf is DeepMid1, DeepMid2 {
    function f() external override {}
}

// ==== Valid: interface diamond resolved by single implementation ====
interface IDiamond {
    function g() external;
}
interface IDiamondA is IDiamond {}
interface IDiamondB is IDiamond {}
contract DiamondImpl is IDiamondA, IDiamondB {
    function g() external {}
}

// ==== Invalid: unresolved diamond with two implementations ====
interface IUnresolved {
    function h() external;
}
contract UnresolvedA is IUnresolved {
    function h() external virtual {}
}
contract UnresolvedB is IUnresolved {
    function h() external virtual {}
}
contract Bad4 is UnresolvedA, UnresolvedB {}
//~^ ERROR: derived contract must override function "h"
