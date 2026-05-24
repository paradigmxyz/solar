//@compile-flags: -Ztypeck

// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_implemented_and_unimplemented_with_implemented_call_via_contract.sol

contract Base {
    function internalBase(uint256 value) internal pure returns (uint256) {
        return value;
    }

    function publicBase(uint256 value) public pure returns (uint256) {
        return value;
    }

    function externalBase(uint256 value) external pure returns (uint256) {
        return value;
    }
}

contract Derived is Base {
    function baseTypeInternalFunction() public pure returns (uint256) {
        return Base.internalBase(1);
    }

    function baseTypePublicFunction() public pure returns (uint256) {
        return Base.publicBase(1);
    }

    function baseTypeExternalFunction() public pure returns (uint256) {
        return Base.externalBase(1); //~ ERROR: cannot call function via contract type name
    }
}

contract ImplementedBase {
    function abstractBaseFunction() public virtual {}
}

abstract contract AbstractBase {
    function abstractBaseFunction() public virtual;
}

contract DerivedFromAbstract is ImplementedBase, AbstractBase {
    function abstractBaseFunction() public override(ImplementedBase, AbstractBase) {
        AbstractBase.abstractBaseFunction(); //~ ERROR: cannot call function via contract type name
    }
}
