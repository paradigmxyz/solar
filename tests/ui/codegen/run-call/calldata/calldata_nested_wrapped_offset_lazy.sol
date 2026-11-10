//@ codegen-matrix: standard
//@ run-call: 0x6bd5555300000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5c => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: 0xbf810a8e00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5c => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: 0x6bd5555300000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1c => 0x0000000000000000000000000000000000000000000000000000000000000000

// Reading the length of a dynamic field through a wrapped pointer stays lazy and returns zero, as
// solc does: only a copy to memory checks the address as an unsigned value.
contract C {
    struct S {
        uint256 a;
        bytes b;
    }

    struct T {
        uint256 a;
        uint256[] v;
    }

    function blen(S[] calldata items) external pure returns (uint256) {
        return items[0].b.length;
    }

    function vlen(T[] calldata items) external pure returns (uint256) {
        return items[0].v.length;
    }
}
