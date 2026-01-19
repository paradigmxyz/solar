// Tests for diamond inheritance with public state variables
// Based on solc tests: public_vars_multiple_diamond*.sol, diamond_*.sol

// ==== Valid: public var can override single interface function ====
interface ISimple {
    function f() external returns (uint);
}
abstract contract VarOverridesInterface is ISimple {
    uint public f;
}

// ==== Valid: shared base with intermediate implementing via public var ====
interface IBase {
    function g() external returns (uint);
}
abstract contract ImplA is IBase {
    uint public g;
}
abstract contract ImplB is IBase {}
// This is fine because A.g is the only implementation below IBase
abstract contract DiamondValid is ImplA, ImplB {}

// ==== Invalid: diamond - public var needs to specify all overridden contracts ====
contract FuncBase {
    function foo() external virtual view returns(uint) { return 5; }
}
contract FuncB is FuncBase {
    function foo() external virtual override view returns(uint) { return 5; }
}
contract FuncC is FuncBase {
    function foo() external virtual override view returns(uint) { return 5; }
}
contract Bad1 is FuncB, FuncC {
    uint public override foo;
    //~^ ERROR: Public state variable needs to specify overridden contracts
}

// ==== Invalid: wrong contracts in override list ====
contract FuncD is FuncBase {
    function foo2() external virtual view returns(uint) { return 6; }
}
contract FuncE is FuncBase {
    function foo2() external virtual view returns(uint) { return 7; }
}
contract Bad2 is FuncD, FuncE {
    uint public override(FuncBase, FuncE) foo2;
    //~^ ERROR: Public state variable needs to specify overridden contracts
    //~| ERROR: invalid contract specified in override list
}

// ==== Invalid: base contract (not interface) with implemented function ====
contract ImplBase {
    function x() external view virtual returns (uint) { return 1; }
}
contract ImplVarA is ImplBase {
    uint public override x;
}
contract ImplVarB is ImplBase {}
contract Bad3 is ImplVarA, ImplVarB {}
//~^ ERROR: derived contract must override function "x"
