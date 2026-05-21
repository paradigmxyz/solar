//@ignore-host: windows
//@compile-flags: --emit=mir

contract Base {
    uint256 public value;

    constructor(uint256 initialValue) {
        value = initialValue;
    }
}

contract Derived is Base {
    constructor(uint256 initialValue) Base(initialValue) {}
}
