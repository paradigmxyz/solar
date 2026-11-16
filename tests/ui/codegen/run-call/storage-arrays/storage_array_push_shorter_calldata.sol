//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: pushShorterCalldata [5, 6] => 5, 6, 0
//@ run-call: pushNarrowCalldata [7, 8] => 7, 8, 0
//@ run-call: pushNestedShorterCalldata [[1, 2], [3, 4]] => 1, 2, 0, 3, 4, 0
//@ run-call: pushExactCalldata [9, 10] => 9, 10

// A calldata argument is materialized at its own type, so pushing a shorter
// fixed array reads only the elements the argument has and zero-fills the
// destination's remaining ones.
contract StorageArrayPushShorterCalldata {
    uint256[3][] private triples;
    uint256[2][] private pairs;
    uint256[3][2][] private grids;

    function pushShorterCalldata(uint256[2] calldata pair)
        external
        returns (uint256, uint256, uint256)
    {
        triples.push(pair);
        return (triples[0][0], triples[0][1], triples[0][2]);
    }

    function pushNarrowCalldata(uint8[2] calldata narrow)
        external
        returns (uint256, uint256, uint256)
    {
        triples.push(narrow);
        return (triples[0][0], triples[0][1], triples[0][2]);
    }

    function pushNestedShorterCalldata(uint256[2][2] calldata grid)
        external
        returns (uint256, uint256, uint256, uint256, uint256, uint256)
    {
        grids.push(grid);
        return (
            grids[0][0][0],
            grids[0][0][1],
            grids[0][0][2],
            grids[0][1][0],
            grids[0][1][1],
            grids[0][1][2]
        );
    }

    // The exact-length path is unchanged: every element comes from calldata.
    function pushExactCalldata(uint256[2] calldata pair) external returns (uint256, uint256) {
        pairs.push(pair);
        return (pairs[0][0], pairs[0][1]);
    }
}
