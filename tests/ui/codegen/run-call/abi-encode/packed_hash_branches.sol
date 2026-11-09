//@ codegen-matrix: standard
//@ run-call: prefix true, "1.2.3" => "v1.2.3"
//@ run-call: prefix false, "1.2.3" => "V1.2.3"
//@ run-call: hash true => 0x3ac225168df54212a25c1c01fd35bebfea408fdac2e31ddd6f80a4bbf9a5f1cb
//@ run-call: hash false => 0xb5553de315e0edf504d9150af82dafa5c4667fa618ed0a6f19c69b41166c5510

contract PackedHashBranches {
    function prefix(bool lower, string memory version) external pure returns (string memory) {
        if (lower) return string(abi.encodePacked("v", version));
        return string(abi.encodePacked("V", version));
    }

    function hash(bool first) external pure returns (bytes32) {
        if (first) return keccak256(abi.encodePacked("a"));
        return keccak256(abi.encodePacked("b"));
    }
}
