//@ codegen-matrix: standard
//@ run-call: DirtyBoolInternalReturn::readDirty => true, 3
//@ run-call: DirtyBoolInternalReturn::join true => 1
//@ run-call: DirtyBoolInternalReturn::join false => 3
//@ run-call: DirtyBoolInternalReturn::reverseJoin true => 3
//@ run-call: DirtyBoolInternalReturn::reverseJoin false => 1
//@ run-call: DirtyBoolInternalReturn::chooseValue true => 1
//@ run-call: DirtyBoolInternalReturn::chooseValue false => 3
//@ run-call: DirtyBoolInternalReturn::loopValue 0 => 1
//@ run-call: DirtyBoolInternalReturn::loopValue 2 => 3

contract DirtyBoolInternalReturn {
    function dirty(bool value) internal pure returns (bool result) {
        assembly {
            result := mul(value, 3)
        }
    }

    function readDirty() external pure returns (bool equal, uint256 raw) {
        bool value = dirty(true);
        equal = value == true;
        assembly {
            raw := value
        }
    }

    function join(bool choose) external pure returns (uint256 raw) {
        bool value;
        if (choose) {
            value = true;
        } else {
            assembly { value := 3 }
        }
        assembly { raw := value }
    }

    function reverseJoin(bool choose) external pure returns (uint256 raw) {
        bool value;
        if (choose) {
            assembly { value := 3 }
        } else {
            value = true;
        }
        assembly { raw := value }
    }

    function chooseValue(bool choose) external pure returns (uint256 raw) {
        bool value = choose ? true : dirty(true);
        assembly { raw := value }
    }

    function loopValue(uint256 n) external pure returns (uint256 raw) {
        bool value = true;
        for (uint256 i; i < n; ++i) {
            assembly { value := add(value, 1) }
        }
        assembly { raw := value }
    }
}
