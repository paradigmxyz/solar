//@ codegen-matrix: standard
//@ run-call: sum 0 => 36
//@ run-call: sum 1 => 37
//@ run-call: sum 255 => 291
contract NarrowModulusCache {
    function sum(uint8 x) external pure returns (uint256 result) {
        assembly {
            result := addmod(x, 1, not(0))
            result := addmod(result, 2, not(0))
            result := addmod(result, 3, not(0))
            result := addmod(result, 4, not(0))
            result := addmod(result, 5, not(0))
            result := addmod(result, 6, not(0))
            result := addmod(result, 7, not(0))
            result := addmod(result, 8, not(0))
        }
    }
}
