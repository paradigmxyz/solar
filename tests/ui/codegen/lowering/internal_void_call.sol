//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract InternalVoidCall {
    uint256 public value;

    function set(uint256 newValue) public {
        writeIfNonZero(newValue);
    }

    function setUnlessZero(uint256 newValue) public {
        if (newValue == 0) {
            return;
        }
        value = newValue;
    }

    function writeIfNonZero(uint256 newValue) internal {
        if (newValue != 0) {
            value = newValue;
        }
    }
}
