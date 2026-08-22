//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: test() => 820, true
// ported-from: test/libsolidity/semanticTests/storage/storage_boundary_struct_array_packed.sol

contract StorageBoundaryStructArrayPacked {
    struct S {
        uint64 a;
        uint64 b;
        uint64 c;
        uint64 d;
    }

    struct Canary {
        uint256 value;
    }

    function getBoundaryArray() internal pure returns (S[10][1] storage arr) {
        assembly {
            arr.slot := sub(0, 5)
        }
    }

    function getDest() internal pure returns (S[10][1] storage arr) {
        assembly {
            arr.slot := 6
        }
    }

    function getCanary() internal pure returns (Canary storage canary) {
        assembly {
            canary.slot := 5
        }
    }

    function test() public returns (uint256 sum, bool canaryIntact) {
        Canary storage canary = getCanary();
        canary.value = type(uint256).max;

        S[10][1] storage source = getBoundaryArray();
        for (uint256 i = 0; i < 10; ++i) {
            source[0][i] = S({
                a: uint64(1 + i * 4),
                b: uint64(2 + i * 4),
                c: uint64(3 + i * 4),
                d: uint64(4 + i * 4)
            });
        }

        S[10][1] storage dest = getDest();
        dest[0] = source[0];
        delete source[0];

        for (uint256 i = 0; i < 10; ++i) {
            sum += dest[0][i].a + dest[0][i].b + dest[0][i].c + dest[0][i].d;
        }
        canaryIntact = canary.value == type(uint256).max && source[0][9].d == 0;
    }
}
