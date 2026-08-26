//@ codegen-matrix: standard
//@ run-call: Chain::ctorNested() => 7, 3, 2
//@ run-call: Chain::runtimeNested 4 => 4, 1
//@ run-call: Chain::storageToStorage() => 7, 9, 3

// Copying a storage array by value chains its materialization loop into a
// second storage-copy loop. The materialized base pointer is a stack-phi
// source that stays live past the loop edge; its spill store must still
// happen or the second loop reloads an uninitialized slot and copies from
// the wrong memory.

contract Chain {
    uint256[] internal xs;
    uint256[][] internal nested;
    uint256[] internal ys;

    constructor() {
        xs.push(7);
        xs.push(8);
        xs.push(9);
        nested.push(xs);
        xs.pop();
    }

    function ctorNested() external view returns (uint256, uint256, uint256) {
        return (nested[0][0], nested[0].length, xs.length);
    }

    function runtimeNested(uint256 value) external returns (uint256, uint256) {
        xs.push(value);
        nested.push(xs);
        uint256 last = nested.length - 1;
        return (nested[last][2], nested[last].length - 2);
    }

    function storageToStorage() external returns (uint256, uint256, uint256) {
        ys = xs;
        return (ys[0], ys[1] + 1, ys.length + 1);
    }
}
