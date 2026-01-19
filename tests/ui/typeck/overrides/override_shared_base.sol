// Tests for shared base scenarios
// Based on solc tests: override_shared_base*.sol

// ==== Valid: shared non-virtual base (C3 linearization resolves) ====
contract SharedNonVirtual {
    function f() external {}
}
contract SharedA is SharedNonVirtual {}
contract SharedB is SharedNonVirtual {}
contract GoodShared is SharedA, SharedB {}

// ==== Valid: shared virtual base with override in diamond ====
contract SharedVirtual {
    function set() public virtual {}
}
contract VirtualA is SharedVirtual {
    uint a;
    function set() public virtual override { a = 1; super.set(); a = 2; }
}
contract VirtualB is SharedVirtual {
    uint b;
    function set() public virtual override { b = 1; super.set(); b = 2; }
}
contract GoodVirtualDiamond is VirtualA, VirtualB {
    function set() public override(VirtualA, VirtualB) { super.set(); }
}

// ==== Valid: partial override - only one branch overrides ====
contract PartialBase {
    function f() external virtual {}
}
contract PartialA is PartialBase {
    function f() external virtual override {}
}
contract PartialB is PartialBase {}
contract GoodPartial is PartialA, PartialB {
    function f() external override(PartialA, PartialBase) {}
}

// ==== Invalid: missing override in diamond ====
contract Bad1 is VirtualA, VirtualB {}
//~^ ERROR: derived contract must override function "set"

// ==== Invalid: incomplete override list in diamond ====
contract Bad2 is VirtualA, VirtualB {
    function set() public override(VirtualA) { super.set(); }
    //~^ ERROR: Function needs to specify overridden contracts
}

// ==== Valid: three-way diamond with shared base ====
contract ThreeWayBase {
    function f() public virtual {}
}
contract ThreeWayA is ThreeWayBase {
    function f() public virtual override {}
}
contract ThreeWayB is ThreeWayBase {
    function f() public virtual override {}
}
contract ThreeWayC is ThreeWayBase {
    function f() public virtual override {}
}
contract GoodThreeWay is ThreeWayA, ThreeWayB, ThreeWayC {
    function f() public override(ThreeWayA, ThreeWayB, ThreeWayC) {}
}

// ==== Invalid: three-way diamond without proper override ====
contract Bad3 is ThreeWayA, ThreeWayB, ThreeWayC {
    function f() public override(ThreeWayA, ThreeWayB) {}
    //~^ ERROR: Function needs to specify overridden contracts
}

// ==== Valid: deep shared base ====
contract DeepBase {
    function g() public virtual {}
}
contract DeepA is DeepBase {}
contract DeepB is DeepBase {}
contract DeepAA is DeepA {}
contract DeepBB is DeepB {}
contract GoodDeep is DeepAA, DeepBB {
    function g() public override {}
}

// ==== Invalid: unresolved at deep level ====
contract DeepImplA is DeepBase {
    function g() public virtual override {}
}
contract DeepImplB is DeepBase {
    function g() public virtual override {}
}
contract DeepImplAA is DeepImplA {}
contract DeepImplBB is DeepImplB {}
contract Bad4 is DeepImplAA, DeepImplBB {}
//~^ ERROR: derived contract must override function "g"
