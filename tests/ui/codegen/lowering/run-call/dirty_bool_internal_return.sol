//@ codegen-matrix: standard
//@ run-call: DirtyBoolInternalReturn::switchCallResult => 11
//@ run-call: DirtyBoolInternalReturn::switchBool true => 11
//@ run-call: DirtyBoolInternalReturn::switchBool false => 7
//@ run-call: DirtyBoolInternalReturn::switchDirty => 13
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
    function switchCallResult() external returns (uint256 result) {
        (bool success,) = address(0).call("");
        assembly {
            switch success
            case 0 { result := 7 }
            default { result := 11 }
        }
    }

    function switchBool(bool value) external pure returns (uint256 result) {
        assembly {
            switch value
            case 0 { result := 7 }
            default { result := 11 }
        }
    }

    function switchDirty() external pure returns (uint256 result) {
        bool value = dirty(true);
        assembly {
            switch value
            case 0 { result := 7 }
            case 1 { result := 11 }
            default { result := 13 }
        }
    }
}
