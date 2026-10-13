//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: run 7, 0, 0 => 7
//@ run-call: run 7, 0, 3 => 28
//@ run-call: run 7, 1024, 3 => 28
//@ run-call: run 9, 4096, 8 => 81

contract RecursiveForwarding {
    function run(uint256 value, uint256 length, uint256 depth) external pure returns (uint256 result) {
        assembly {
            function recurse(v, n, d) -> out {
                calldatacopy(0x80, calldatasize(), n)
                if d {
                    let inner := recurse(v, n, sub(d, 1))
                    out := add(v, inner)
                    leave
                }
                out := v
            }
            result := recurse(value, length, depth)
        }
    }
}
