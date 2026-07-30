//@ run-call: Derived::pointer() => 2

contract Base {
    function value(uint256 x) internal pure virtual returns (uint256) {
        return x + 1;
    }
}

contract Derived is Base {
    function value(uint256 x) internal pure override returns (uint256) {
        return x + 100;
    }

    function pointer() external pure returns (uint256) {
        function(uint256) internal pure returns (uint256) target = Base.value;
        return target(1);
    }
}
