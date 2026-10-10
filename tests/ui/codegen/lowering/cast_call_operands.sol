//@ codegen-matrix: standard
//@ run-call: Caller::castCallOperands false => 14
//@ run-call: Caller::castCallOperands true => 12

struct CallConfig {
    uint160 firstValue;
    uint160 secondValue;
    int24 lower;
    int24 upper;
    uint256 firstAmount;
    uint256 secondAmount;
}

struct CallArguments {
    address first;
    address second;
    int24 lower;
    int24 upper;
    uint256 firstAmount;
    uint256 secondAmount;
}

contract CallTarget {
    uint160 private prepared;

    function accept(address, uint256) external pure returns (bool) {
        return true;
    }

    function prepare(address, address, uint24, uint160 value) external returns (address) {
        prepared = value;
        return address(this);
    }

    function consume(CallArguments calldata params)
        external view returns (uint256, uint128, uint256, uint256)
    {
        return (params.secondAmount + prepared, 0, 0, 0);
    }
}

contract Caller {
    CallTarget private target;
    mapping(uint256 => CallConfig) private callConfigs;
    mapping(address => uint256) private callResults;

    constructor() {
        target = new CallTarget();
    }

    function castCallOperands(bool reverse) external returns (uint256) {
        address a = address(target);
        address b = address(new CallTarget());
        (address low, address high) = a < b ? (a, b) : (b, a);
        callConfigs[0] = CallConfig(3, 5, -2, 4, 7, 11);
        a = reverse ? high : low;
        b = reverse ? low : high;
        invokeWideCall(a, b, 0);
        return callResults[a];
    }

    function invokeWideCall(address a, address b, uint256 configIndex) internal {
        CallConfig memory config = callConfigs[configIndex];
        bool firstIsLower = a < b;

        address first = firstIsLower ? a : b;
        address second = firstIsLower ? b : a;

        CallTarget(payable(first)).accept(address(target), type(uint256).max);
        CallTarget(payable(second)).accept(address(target), type(uint256).max);

        CallTarget manager = target;

        uint160 value = firstIsLower ? config.firstValue : config.secondValue;

        int24 lower;
        int24 upper;

        if (firstIsLower) {
            lower = config.lower;
            upper = config.upper;
        } else {
            lower = -config.upper;
            upper = -config.lower;
        }

        uint256 firstAmount = firstIsLower ? config.firstAmount : config.secondAmount;
        uint256 secondAmount = firstIsLower ? config.secondAmount : config.firstAmount;

        manager.prepare(first, second, 10000, value);

        (uint256 result,,,) = manager.consume(
            CallArguments({
                first: first,
                second: second,
                lower: lower,
                upper: upper,
                firstAmount: firstAmount,
                secondAmount: secondAmount
            })
        );

        callResults[a] = result;
    }
}
