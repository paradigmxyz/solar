//@compile-flags: -Ztypeck

// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/065_super_excludes_current_contract.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/super_on_external.sol
// ported-from: test/libsolidity/syntaxTests/super/unimplemented_super_function.sol

contract A {
    function externalOnly() external {}
}

abstract contract AbstractBase {
    function unimplemented() public virtual returns (uint256);
}

abstract contract B is A, AbstractBase {
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
}
