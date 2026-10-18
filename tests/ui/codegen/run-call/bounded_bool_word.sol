//@ codegen-matrix: standard
//@ run-call: fromWord 0 => false
//@ run-call: fromWord 1 => true
//@ run-call: fromWord 2 => true
//@ run-call-fail: fromWord 3

// The guard bounds the word below 3, not 2, so encoding the boolean must
// still normalize the word 2 to 1.
contract BoundedBoolWord {
    function fromWord(uint256 word) external pure returns (bool flag) {
        require(word < 3);
        assembly {
            flag := word
        }
    }
}
