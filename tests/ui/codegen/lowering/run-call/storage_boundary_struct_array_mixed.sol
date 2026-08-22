//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: test() => 1000, true
// ported-from: test/libsolidity/semanticTests/storage/storage_boundary_struct_array_mixed_types.sol

contract StorageBoundaryStructArrayMixed {
    struct S {
        uint256 a;
        uint128 b;
        uint64 c;
        bytes32 d;
        bool e;
    }

    struct Canary {
        uint256 value;
    }

    function getBoundaryArray() internal pure returns (S[10][1] storage arr) {
        assembly {
            arr.slot := sub(0, 20)
        }
    }

    function getDest() internal pure returns (S[10][1] storage arr) {
        assembly {
            arr.slot := 21
        }
    }

    function getCanary() internal pure returns (Canary storage canary) {
        assembly {
            canary.slot := 20
        }
    }

    function test() public returns (uint256 sum, bool canaryIntact) {
        Canary storage canary = getCanary();
        canary.value = type(uint256).max;

        S[10][1] storage source = getBoundaryArray();
        for (uint256 i = 0; i < 10; ++i) {
            source[0][i] = S({
                a: 1 + i * 5,
                b: uint128(2 + i * 5),
                c: uint64(3 + i * 5),
                d: bytes32(uint256(4 + i * 5)),
                e: true
            });
        }

        S[10][1] storage dest = getDest();
        dest[0] = source[0];
        delete source[0];

        canaryIntact = canary.value == type(uint256).max && source[0][9].d == bytes32(0);
        for (uint256 i = 0; i < 10; ++i) {
            sum += dest[0][i].a;
            sum += uint256(dest[0][i].b);
            sum += uint256(dest[0][i].c);
            sum += uint256(dest[0][i].d);
            canaryIntact = canaryIntact && dest[0][i].e;
        }
    }
}
