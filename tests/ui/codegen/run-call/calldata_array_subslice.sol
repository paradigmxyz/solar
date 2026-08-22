//@ run-call: word [1, 2, 3, 4] => [2, 3, 4]

contract CalldataArraySubsliceRuntime {
    // Range access on a word-element calldata array keeps the slice lazy: the
    // adjusted pointer and shortened length rebuild the memory array on return.
    function word(uint256[] calldata a) external pure returns (uint256[] memory) {
        return a[1:];
    }
}
