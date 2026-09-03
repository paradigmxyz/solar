//@ codegen-matrix: standard
//@ run-call: RecursiveStaticFrameArgs::permute 12, 34, 0 => 12034
//@ run-call: RecursiveStaticFrameArgs::permute 12, 34, 1 => 34012
//@ run-call: RecursiveStaticFrameArgs::permute 12, 34, 2 => 12034

contract RecursiveStaticFrameArgs {
    function permute(uint256 a, uint256 b, uint256 depth)
        external
        pure
        returns (uint256 result)
    {
        assembly {
            function recurse(x, y, remaining) -> out {
                if remaining {
                    out := recurse(y, x, sub(remaining, 1))
                    leave
                }
                out := add(mul(x, 1000), y)
            }
            result := recurse(a, b, depth)
        }
    }
}
