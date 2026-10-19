//@ codegen-matrix: standard
//@ run-call: one 5 => 6
//@ run-call: two 5 => 5, 6
//@ run-call: three 5 => 5, 6, 7
//@ run-call: unused 5 =>

// Static results of at most two words are encoded in scratch memory. A
// three-word result needs the buffer at `0x80`, above the free memory pointer.
contract StaticReturnScratch {
    function one(uint256 x) external pure returns (uint256) {
        return x + 1;
    }

    function two(uint256 x) external pure returns (uint256, uint256) {
        return (x, x + 1);
    }

    function three(uint256 x) external pure returns (uint256, uint256, uint256) {
        return (x, x + 1, x + 2);
    }

    function unused(uint256) external pure {}
}
