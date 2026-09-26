//@ revisions: none gas size
//@ compile-flags: --emit=bin -Zvalidate-ir=true
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//~[none]? ERROR: codegen cannot preserve values across a low-memory forwarding buffer
//~[gas,size]? ERROR: codegen cannot preserve arguments across a low-memory write

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
