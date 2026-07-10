// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_shared_base.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_shared_base_partial.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_shared_base_simple.sol

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
