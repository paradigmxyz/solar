//@ codegen-matrix: standard
//@ run-call: DirtyBoolInternalReturn::readDirty() => true, 3

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
}
