//@ compile-flags: -Ztypeck

// ported-from: test/libsolidity/semanticTests/functionTypes/selector_1.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/484_function_types_selector_1.sol
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

    function ownPublicFunctionSelector() public pure returns (bytes4) {
        return publicBase.selector; //~ ERROR: member `selector` not found
    }
}

contract Derived is Base {
    function inheritedPublicFunctionSelector() public pure returns (bytes4) {
        return publicBase.selector;
    }

    function inheritedInternalFunctionSelector() public pure returns (bytes4) {
        return internalBase.selector; //~ ERROR: member `selector` not found
    }

    function selfTypePublicFunctionSelector() public pure returns (bytes4) {
        return Derived.baseTypePublicFunction.selector; //~ ERROR: member `selector` not found
    }

    function baseTypePublicFunctionSelector() public pure returns (bytes4) {
        return Base.publicBase.selector;
    }

    function baseTypeInternalFunction() public pure returns (uint256) {
        return Base.internalBase(1);
    }

    function baseTypePublicFunction() public pure returns (uint256) {
        return Base.publicBase(1);
    }

    function baseTypePublicFunctionPointer() public pure returns (uint256) {
        function(uint256) internal pure returns (uint256) pointer = Base.publicBase;
        return pointer(1);
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
