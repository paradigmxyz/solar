//@ run-call: Derived::exact() => 2
//@ run-call: Derived::exactPointer() => 2
//@ run-call: Derived::dynamic() => 101
//@ run-call: Concrete::through() => 7
//@ run-call: SuperDerived::direct() => 11
//@ run-call: SuperDerived::parenthesized() => 11
//@ run-call: SuperDerived::pointer() => 11
//@ run-call: SuperDerived::dynamic() => 101
//@ run-call: SkipUnimplemented::callSuper() => 42
// ported-from: test/libsolidity/semanticTests/various/super.sol
// ported-from: test/libsolidity/semanticTests/various/super_parentheses.sol
// ported-from: test/libsolidity/semanticTests/functionCall/inheritance/super_skip_unimplemented_in_abstract_contract.sol

contract Base {
    function value(uint256 x) internal pure virtual returns (uint256) {
        return x + 1;
    }
}

contract Derived is Base {
    function value(uint256 x) internal pure override returns (uint256) {
        return x + 100;
    }

    function exact() external pure returns (uint256) {
        return Base.value(1);
    }

    function exactPointer() external pure returns (uint256) {
        function(uint256) internal pure returns (uint256) target = Base.value;
        return target(1);
    }

    function dynamic() external pure returns (uint256) {
        return value(1);
    }
}

abstract contract Abstract {
    function hook() public pure virtual returns (uint256);

    function through() public pure returns (uint256) {
        return hook();
    }
}

contract Concrete is Abstract {
    function hook() public pure override returns (uint256) {
        return 7;
    }
}

contract SuperMiddle is Base {
    function value(uint256 x) internal pure virtual override returns (uint256) {
        return x + 10;
    }
}

contract SuperDerived is SuperMiddle {
    function value(uint256 x) internal pure override returns (uint256) {
        return x + 100;
    }

    function direct() external pure returns (uint256) {
        return super.value(1);
    }

    function parenthesized() external pure returns (uint256) {
        return ((super).value)(1);
    }

    function pointer() external pure returns (uint256) {
        function(uint256) internal pure returns (uint256) target = super.value;
        return target(1);
    }

    function dynamic() external pure returns (uint256) {
        return value(1);
    }
}

contract ImplementedBase {
    function getValue() public pure virtual returns (uint256) {
        return 42;
    }
}

abstract contract UnimplementedBase {
    function getValue() external pure virtual returns (uint256);
}

contract SkipUnimplemented is ImplementedBase, UnimplementedBase {
    function getValue()
        public
        pure
        override(ImplementedBase, UnimplementedBase)
        returns (uint256)
    {
        return super.getValue();
    }

    function callSuper() external pure returns (uint256) {
        return super.getValue();
    }
}
