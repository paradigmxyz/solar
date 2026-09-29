//@ revisions: none gas size
//@ compile-flags: --emit=bin -Zvalidate-ir=true
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//~[none]? ERROR: codegen cannot preserve values across a low-memory forwarding buffer
//~[none]? ERROR: codegen cannot preserve values across a low-memory forwarding buffer in `computedForward`
//~[gas,size]? ERROR: codegen cannot preserve arguments across a low-memory write
//~[gas,size]? ERROR: codegen cannot preserve arguments across a low-memory write in `copyLoop`

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

    function copy(address target, uint256 length) external view returns (uint256 total) {
        uint256 dest;
        assembly {
            dest := mload(0x40)
        }
        total = copyLoop(target, dest, length);
        assembly {
            return(dest, total)
        }
    }

    function copyLoop(address target, uint256 dest, uint256 length) private view returns (uint256 total) {
        assembly {
            let at := dest
            for { let i := 0 } lt(i, length) { i := add(i, 1) } {
                let size := extcodesize(target)
                extcodecopy(target, at, 0, size)
                at := add(at, size)
            }
            total := sub(at, dest)
        }
    }

    function computed(uint256 value, uint256 length) external pure returns (uint256) {
        return computedForward(value, length);
    }

    function computedForward(uint256 value, uint256 length) private pure returns (uint256) {
        uint256 saved = value * 3;
        assembly { calldatacopy(0x80, calldatasize(), length) }
        if (length != 0) return consume(value) + saved;
        return saved;
    }

    function consume(uint256 value) private pure returns (uint256) {
        return value + 1;
    }
}
