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
//@[none, gas, size] run-call: allocate => 7
// ported-from: test/libsolidity/semanticTests/array/create_multiple_dynamic_arrays.sol

contract MemoryMultipleDynamicArrays {
    function allocate() external pure returns (uint256) {
        uint256[][] memory first = new uint256[][](42);
        first[0] = new uint256[](1);
        first[0][0] = 1;
        first[4] = new uint256[](1);
        first[4][0] = 2;
        first[10] = new uint256[](1);
        first[10][0] = 44;

        uint256[][] memory second = new uint256[][](24);
        second[0] = new uint256[](1);
        second[0][0] = 1;
        second[4] = new uint256[](1);
        second[4][0] = 2;
        second[10] = new uint256[](1);
        second[10][0] = 88;

        if (
            first[0][0] == second[0][0]
                && first[4][0] == second[4][0]
                && first[10][0] == 44
                && second[10][0] == 88
        ) {
            return 7;
        }
        return 0;
    }
}
