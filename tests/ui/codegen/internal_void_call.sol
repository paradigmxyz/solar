//@ignore-host: windows
//@compile-flags: --emit=mir

contract InternalVoidCall {
    uint256 public value;

    function set(uint256 newValue) public {
        writeIfNonZero(newValue);
    }

    function writeIfNonZero(uint256 newValue) internal {
        if (newValue != 0) {
            value = newValue;
        }
    }
}
