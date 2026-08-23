//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: t() => 9
// ported-from: test/libsolidity/semanticTests/functionTypes/store_function.sol

contract ExternalFunctionPointerHigherOrderTarget {
    function addTwo(uint256 value) external pure returns (uint256) {
        return value + 2;
    }
}

contract ExternalFunctionPointerHigherOrder {
    function(function(uint256) external returns (uint256)) internal returns (uint256) evaluator;
    function(uint256) external returns (uint256) target;

    function store(function(uint256) external returns (uint256) value) public {
        target = value;
    }

    function eval(function(uint256) external returns (uint256) value)
        public
        returns (uint256)
    {
        return value(7);
    }

    function t() external returns (uint256) {
        evaluator = eval;
        this.store((new ExternalFunctionPointerHigherOrderTarget()).addTwo);
        return evaluator(target);
    }
}
