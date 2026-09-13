//@ codegen-matrix: standard
//@ run-call: MutualRecursiveYulFrames::encode 0 => 1, 1, 0xc000000000000000000000000000000000000000000000000000000000000000
//@ run-call: MutualRecursiveYulFrames::encode 3 => 4, 4, 0xa3a2a1c000000000000000000000000000000000000000000000000000000000

//@ run-call: MutualRecursiveYulFrames::arithmetic 0 => 7, 1
//@ run-call: MutualRecursiveYulFrames::arithmetic 3 => 13, 4

contract MutualRecursiveYulFrames {
    function encode(uint256 depth)
        external
        pure
        returns (uint256 length, uint256 count, bytes32 word)
    {
        assembly {
            function walk(n, out) -> end, items {
                if iszero(n) {
                    mstore8(out, 0xc0)
                    end := add(out, 1)
                    items := 1
                    leave
                }
                end, items := container(n, out)
            }
            function container(n, out) -> end, items {
                end, items := walk(sub(n, 1), add(out, 1))
                mstore8(out, add(0xa0, n))
                items := add(items, 1)
            }
            let out := mload(0x40)
            let end, items := walk(depth, out)
            length := sub(end, out)
            count := items
            word := mload(out)
        }
    }

    function arithmetic(uint256 depth) external pure returns (uint256 sum, uint256 count) {
        /// @solidity memory-safe-assembly
        assembly {
            function walk(n, acc) -> total, items {
                switch n
                case 0 {
                    total := acc
                    items := 1
                }
                default { total, items := container(n, acc) }
            }
            function container(n, acc) -> total, items {
                total, items := walk(sub(n, 1), addSteps(n, acc))
                items := add(items, 1)
            }
            function addSteps(n, acc) -> total {
                total := acc
                for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                    total := add(total, 1)
                }
            }
            sum, count := walk(depth, 7)
        }
    }

}
