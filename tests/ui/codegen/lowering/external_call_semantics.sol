//@ run-call: Caller::lowLevelGas() => false
//@ run-call: Caller::highLevelGas() => true
//@ run-call: Caller::viewUsesStaticcall() => true
//@ run-call: Caller::attachedStorageReceiver() => 7
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

    function attachedStorageReceiver() external returns (uint256) {
        data.set(7);
        return data.value;
    }

    function failedCreation() external {
        new FailingConstructor();
    }
}
