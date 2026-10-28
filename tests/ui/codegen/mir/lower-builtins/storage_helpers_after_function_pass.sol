//@ compile-flags: -Ogas --emit=bin
//@ compile-flags: -Zmir-pipeline=sccp,lower-checks,lower-builtins,lower-arithmetic,lower-abi,lower-abi-encode,lower-dispatch,lower-frame-slots,lower-mapping-slots,lower-memory-objects,lower-slices,lower-alloc,lower-memory-zero,lower-evm-shaped
//@ normalize-stdout-test: "[0-9a-f]{40,}" -> "BYTECODE"
// `lower-builtins` adds storage bytes helpers after a function pass priced creation code; the
// creation pricing sets must grow with them instead of failing a bounds check.
contract C {
    string s;

    function set(string memory x) external {
        s = x;
    }

    function get() external view returns (string memory) {
        return s;
    }

    function lit() external {
        s = "hello";
    }
}
