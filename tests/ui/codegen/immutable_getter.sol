//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=mir

contract C {
    address public immutable owner;

    constructor(address value) {
        owner = value;
    }
}
