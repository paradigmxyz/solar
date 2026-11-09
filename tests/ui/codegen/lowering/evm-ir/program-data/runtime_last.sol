//@ compile-flags: -Ogas -Zdump=disasm-deploy

// The deployment code ends with the runtime code, so the data that `pack-data` adds goes before it.
contract RuntimeLast {
    event Seen(bytes data);

    constructor() {
        emit Seen("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef-this string is long enough for a copy-fedcba9876543210fedcba9876543210");
    }

    function f() external {}
}
