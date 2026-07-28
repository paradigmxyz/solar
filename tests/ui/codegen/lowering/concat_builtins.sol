//@ run-call: strings() => 0xa3da165071fd0481760dc90e0f0348f5505219b77abe9f5f3a367ae3a160a731
//@ run-call: blobs() => 0x13a08e3cd39a1bc7bf9103f63f83273cced2beada9f723945176d6b983c65bd2
//@ run-call: empty() => 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470

contract ConcatBuiltins {
    function strings() external pure returns (bytes32) {
        string memory middle = "XY";
        return keccak256(bytes(string.concat("ab", middle, "", "cd")));
    }

    function blobs() external pure returns (bytes32) {
        bytes memory middle = hex"0304";
        return keccak256(bytes.concat(hex"0102", middle, bytes2(0x0506), hex""));
    }

    function empty() external pure returns (bytes32) {
        return keccak256(bytes.concat());
    }
}
