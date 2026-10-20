//@ codegen-matrix: standard
//@ run-call: one 5 => 6
//@ run-call: two 5 => 5, 6
//@ run-call: three 5 => 5, 6, 7
//@ run-call: unused 5 =>
//@ run-call: swapped => 2, 1
//@ run-call: aliasedArrays => [2], [1]
//@ run-call: scalarThenAlias => 5, [7]

// Static results of at most two words are encoded in scratch memory. A
// three-word result needs the buffer at `0x80`, above the free memory pointer.
// Results with memory aggregates keep the buffer: their source can alias scratch.
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

    function swapped() external pure returns (uint256 x, uint256 y) {
        assembly {
            mstore(0, 1)
            mstore(0x20, 2)
            x := mload(0x20)
            y := mload(0)
        }
    }

    function aliasedArrays() external pure returns (uint256[1] memory p, uint256[1] memory q) {
        assembly {
            mstore(0, 1)
            mstore(0x20, 2)
            p := 0x20
            q := 0x00
        }
    }

    function scalarThenAlias() external pure returns (uint256 x, uint256[1] memory q) {
        assembly {
            mstore(0, 7)
            q := 0x00
        }
        x = 5;
    }
}
