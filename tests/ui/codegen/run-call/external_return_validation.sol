//@ run-call-fail: ExternalReturnValidation::short()
//@ run-call-fail: ExternalReturnValidation::dirty()
//@ run-call: ExternalReturnValidation::dirtyValue() => 0
//@ run-call: ExternalReturnValidation::dirtyBool() => true
//@ run-call: ExternalReturnValidation::dirtyStruct() => (0, true)
//@ run-call: ExternalReturnValidation::dirtyArray() => [true, true]

contract ExternalReturnValidation {
    struct Pair {
        uint8 value;
        bool flag;
    }

    function short() external view returns (uint256) {
        return this.shortTarget();
    }

    function shortTarget() external pure returns (uint256) {
        assembly {
            return(0, 0)
        }
    }

    function dirty() external view returns (uint8) {
        return this.dirtyTarget();
    }

    function dirtyTarget() external pure returns (uint8) {
        assembly {
            mstore(0, 0x100)
            return(0, 32)
        }
    }

    function dirtyValue() external pure returns (uint8) {
        uint8 value;
        assembly {
            value := 0x100
        }
        return value;
    }

    function dirtyBool() external pure returns (bool) {
        bool value;
        assembly {
            value := 2
        }
        return value;
    }

    function dirtyStruct() external pure returns (Pair memory pair) {
        assembly {
            mstore(pair, 0x100)
            mstore(add(pair, 0x20), 2)
        }
    }

    function dirtyArray() external pure returns (bool[] memory values) {
        values = new bool[](2);
        assembly {
            mstore(add(values, 0x20), 2)
            mstore(add(values, 0x40), 0x100)
        }
    }
}
