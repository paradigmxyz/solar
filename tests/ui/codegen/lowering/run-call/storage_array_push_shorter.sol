//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: pushShorterMemory => 5, 6, 0
//@ run-call: pushShorterLiteral => 1, 2, 0
//@ run-call: pushNarrowMemory => 7, 8, 0

// A copy into storage converts element-wise, so pushing a shorter fixed array
// copies the elements the argument has and zero-fills the destination's
// remaining ones.
contract StorageArrayPushShorter {
    uint256[3][] private triples;

    function pushShorterMemory() external returns (uint256, uint256, uint256) {
        uint256[2] memory pair = [uint256(5), 6];
        // The word right behind `pair` is not zero, so a copy that reads three
        // elements from a two-element source observes it.
        uint256[2] memory other = [uint256(9), 9];
        other;
        triples.push(pair);
        return (triples[0][0], triples[0][1], triples[0][2]);
    }

    function pushShorterLiteral() external returns (uint256, uint256, uint256) {
        triples.push([1, 2]);
        return (triples[0][0], triples[0][1], triples[0][2]);
    }

    function pushNarrowMemory() external returns (uint256, uint256, uint256) {
        uint8[2] memory narrow = [uint8(7), 8];
        triples.push(narrow);
        return (triples[0][0], triples[0][1], triples[0][2]);
    }
}
