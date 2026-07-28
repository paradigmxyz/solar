//@ run-call: Derived::exact() => 2
//@ run-call: Derived::dynamic() => 101
//@ run-call: Concrete::through() => 7

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
