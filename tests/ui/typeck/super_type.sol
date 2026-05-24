//@compile-flags: -Ztypeck

// ported-from: test/libsolidity/semanticTests/various/super.sol
// ported-from: test/libsolidity/semanticTests/inheritance/super_overload.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/065_super_excludes_current_contract.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/super_on_external.sol
// ported-from: test/libsolidity/syntaxTests/super/unimplemented_super_function.sol
// ported-from: test/libsolidity/syntaxTests/conversion/convert_to_super_empty.sol

contract A {
    function f() public virtual returns (uint256) {
        return 1;
    }

    function overloaded(bool) public virtual returns (uint256) {
        return 2;
    }

    function overloaded(uint256) public virtual returns (uint256) {
        return 3;
    }

    function externalOnly() external {}
}

abstract contract AbstractBase {
    function unimplemented() public virtual returns (uint256);
}

abstract contract B is A, AbstractBase {
    function f() public virtual override returns (uint256) {
        return super.f() + 1;
    }

    function overloaded(bool value) public virtual override returns (uint256) {
        return super.overloaded(value) + 1;
    }

    function overloaded(uint256 value) public virtual override returns (uint256) {
        return super.overloaded(value) + 1;
    }

    function currentOnly() public {}

    function superExcludesCurrent() public {
        super.currentOnly(); //~ ERROR: member `currentOnly` not found
    }

    function superExcludesExternal() public {
        super.externalOnly(); //~ ERROR: member `externalOnly` not found
    }

    function superExcludesUnimplemented() public returns (uint256) {
        return super.unimplemented(); //~ ERROR: member `unimplemented` not found
    }

    function cannotConvertToSuper() public {
        super(); //~ ERROR: cannot convert to the super type
    }

}

abstract contract C is B {
    function f() public override returns (uint256) {
        return super.f() + 1;
    }

    function overloadedFromC(bool value) public returns (uint256) {
        return super.overloaded(value);
    }

    function overloadedFromC(uint256 value) public returns (uint256) {
        return super.overloaded(value);
    }
}
