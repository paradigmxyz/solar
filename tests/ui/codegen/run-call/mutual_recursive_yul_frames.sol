//@ run-call: MutualRecursiveYulFrames::encode 0 => 1, 1, 0xc000000000000000000000000000000000000000000000000000000000000000
//@ run-call: MutualRecursiveYulFrames::encode 3 => 4, 4, 0xa3a2a1c000000000000000000000000000000000000000000000000000000000

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
}
