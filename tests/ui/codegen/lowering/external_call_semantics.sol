//@ codegen-matrix: standard
//@ compile-flags: -Zvalidate-ir=true
//@ run-call: Caller::lowLevelGas => false
//@ run-call: Caller::highLevelGas => true
//@ run-call: Caller::viewUsesStaticcall => true
//@ run-call: Caller::namedArguments => 12
//@ run-call: Caller::internalNamedArguments => 12
//@ run-call: Caller::libraryNamedArguments => 12
//@ run-call: Caller::structNamedArguments => 12
//@ run-call: Caller::attachedStorageReceiver => 7
//@ run-call: Caller::functionPointerMultiReturn => 12
//@ run-call-fail: Caller::failedCreation => 0xdeadbeef
//@ run-call: Caller::castCallOperands false => 14
//@ run-call: Caller::castCallOperands true => 12

interface ViewTarget {
    function touch() external view;
}

contract CallTarget {
    uint160 private prepared;

    fallback() external {
        assembly {
            log0(0, 0)
        }
    }

    function ping() external {}

    function ordered(uint256 a, uint256 b) external pure returns (uint256) {
        return a * 10 + b;
    }

    function pair() external pure returns (uint256, uint256) {
        return (1, 2);
    }

    function touch() external {
        assembly {
            sstore(0, 1)
        }
    }

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

contract FailingConstructor {
    constructor() {
        assembly {
            mstore(0, shl(224, 0xdeadbeef))
            revert(0, 4)
        }
    }
}

struct NamedPair {
    uint256 a;
    uint256 b;
}

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

library NamedCallLib {
    function ordered(uint256 a, uint256 b) internal pure returns (uint256) {
        return a * 10 + b;
    }
}

library StorageLib {
    struct Data {
        uint256 value;
    }

    function set(Data storage self, uint256 value) internal {
        self.value = value;
    }
}

contract Caller {
    using StorageLib for StorageLib.Data;

    CallTarget private target;
    StorageLib.Data private data;
    mapping(uint256 => CallConfig) private callConfigs;
    mapping(address => uint256) private callResults;

    constructor() {
        target = new CallTarget();
    }

    function lowLevelGas() external returns (bool) {
        (bool success,) = address(target).call{gas: 0}("");
        return success;
    }

    function highLevelGas() external returns (bool) {
        try target.ping{gas: 0}() {
            return false;
        } catch {
            return true;
        }
    }

    function viewUsesStaticcall() external view returns (bool) {
        try ViewTarget(address(target)).touch() {
            return false;
        } catch {
            return true;
        }
    }

    function namedArguments() external view returns (uint256) {
        return target.ordered({b: 2, a: 1});
    }

    function orderedInternal(uint256 a, uint256 b) internal pure returns (uint256) {
        return a * 10 + b;
    }

    function internalNamedArguments() external pure returns (uint256) {
        return orderedInternal({b: 2, a: 1});
    }

    function libraryNamedArguments() external pure returns (uint256) {
        return NamedCallLib.ordered({b: 2, a: 1});
    }

    function structNamedArguments() external pure returns (uint256) {
        NamedPair memory pair = NamedPair({b: 2, a: 1});
        return pair.a * 10 + pair.b;
    }

    function attachedStorageReceiver() external returns (uint256) {
        data.set(7);
        return data.value;
    }

    function functionPointerMultiReturn() external view returns (uint256) {
        function() external view returns (uint256, uint256) pointer = target.pair;
        (uint256 a, uint256 b) = pointer();
        return a * 10 + b;
    }

    function failedCreation() external {
        new FailingConstructor();
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
