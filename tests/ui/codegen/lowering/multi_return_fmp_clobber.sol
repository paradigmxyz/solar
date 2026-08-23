//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: run 2 => 215

contract MultiReturnFmpClobber {
    function run(uint256 x) external pure returns (uint256) {
        uint256 keep = x * 3;
        (uint256 first,) = pair(x);
        return keep + first;
    }

    function pair(uint256 x) internal pure returns (uint256 first, uint256 second) {
        unchecked {
            first = x + 1;
            first *= 3;
            first ^= 5;
            first += 7;
            first *= 11;
            second = x + 2;
        }
        assembly {
            mstore(0x40, not(0))
        }
    }
}
