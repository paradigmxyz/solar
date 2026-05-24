//@compile-flags: -Ztypeck

// ported-from: test/libsolidity/semanticTests/various/super.sol
// ported-from: test/libsolidity/semanticTests/various/super_parentheses.sol
// ported-from: test/libsolidity/semanticTests/inheritance/super_overload.sol
// ported-from: test/libsolidity/semanticTests/inheritance/super_in_constructor_assignment.sol
// ported-from: test/libsolidity/semanticTests/functionCall/inheritance/super_skip_unimplemented_in_abstract_contract.sol
// ported-from: test/libsolidity/semanticTests/functionCall/inheritance/super_skip_unimplemented_in_interface.sol

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
}

abstract contract B is A {
    function f() public virtual override returns (uint256) {
        return super.f() + 1;
    }

    function overloaded(bool value) public virtual override returns (uint256) {
        return super.overloaded(value) + 1;
    }

    function overloaded(uint256 value) public virtual override returns (uint256) {
        return super.overloaded(value) + 1;
    }
}

abstract contract C is B {
    function f() public override returns (uint256) {
        return super.f() + 1;
    }

    function viaParentheses() public returns (uint256) {
        return ((super).f)() + 1;
    }

    function viaFunctionPointer() public returns (uint256) {
        function() internal returns (uint256) pointer = super.f;
        return pointer() + 1;
    }

    function overloadedFromC(bool value) public returns (uint256) {
        return super.overloaded(value);
    }

    function overloadedFromC(uint256 value) public returns (uint256) {
        return super.overloaded(value);
    }
}

contract Implemented {
    function skipMe() public virtual returns (uint256) {
        return 42;
    }
}

abstract contract AbstractUnimplemented {
    function skipMe() external virtual returns (uint256);
}

interface InterfaceUnimplemented {
    function skipMe() external returns (uint256);
}

contract SkipAbstract is Implemented, AbstractUnimplemented {
    function skipMe() public override(Implemented, AbstractUnimplemented) returns (uint256) {
        return super.skipMe();
    }
}

contract SkipInterface is Implemented, InterfaceUnimplemented {
    function skipMe() public override(Implemented, InterfaceUnimplemented) returns (uint256) {
        return super.skipMe();
    }
}
