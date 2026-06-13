//@ignore-host: windows
//@compile-flags: --emit=mir

contract C {
    address public immutable owner;

    constructor(address value) {
        owner = value;
    }
}
