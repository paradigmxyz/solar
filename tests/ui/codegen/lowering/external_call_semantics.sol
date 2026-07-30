//@ run-call: Caller::lowLevelGas() => false
//@ run-call: Caller::highLevelGas() => true
//@ run-call: Caller::viewUsesStaticcall() => true
//@ run-call: Caller::namedArguments() => 12
//@ run-call: Caller::internalNamedArguments() => 12
//@ run-call: Caller::libraryNamedArguments() => 12
//@ run-call: Caller::structNamedArguments() => 12
//@ run-call: Caller::attachedStorageReceiver() => 7
//@ run-call: Caller::functionPointerMultiReturn() => 12
//@ run-call-fail: Caller::failedCreation() => 0xdeadbeef

interface ViewTarget {
    function touch() external view;
}

contract CallTarget {
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
}
