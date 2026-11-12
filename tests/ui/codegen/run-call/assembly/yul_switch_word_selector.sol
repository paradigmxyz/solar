//@ codegen-matrix: standard
//@ run-call: switchCallResult => 11
//@ run-call: switchBool true => 11
//@ run-call: switchBool false => 7
//@ run-call: switchDirty => 13

contract YulSwitchWordSelector {
    function dirty(bool value) internal pure returns (bool result) {
        assembly {
            result := mul(value, 3)
        }
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
