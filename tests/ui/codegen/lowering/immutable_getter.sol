//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract C {
    address public immutable owner;

    constructor(address value) {
        owner = value;
    }
}
