//@ compile-flags: -Ztypeck

// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/065_super_excludes_current_contract.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/super_on_external.sol
// ported-from: test/libsolidity/syntaxTests/super/unimplemented_super_function.sol

contract CurrentBase {
    function inherited() public {}
}
contract CurrentDerived is CurrentBase {
    function currentOnly() public {
        super.currentOnly(); //~ ERROR: member `currentOnly` not found
    }
}

contract ExternalBase {
    function f() external virtual pure {}
}
contract ExternalDerived is ExternalBase {
    function f() public override pure {
        super.f(); //~ ERROR: member `f` not found
    }
}

abstract contract AbstractBase {
    function f() public virtual;
}
contract AbstractDerived is AbstractBase {
    function f() public override {
        super.f(); //~ ERROR: member `f` not found
    }
}
