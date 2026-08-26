//@ codegen-matrix: standard
//@ run-call: StorageStringSlot::lengthThroughRef() => 40
//@ run-call: StorageStringSlot::returnThroughRef() => 40

library StorageStringRef {
    function length(string storage value) internal view returns (uint256) {
        return bytes(value).length;
    }

    function read(string storage value) internal pure returns (string memory) {
        return value;
    }
}

contract StorageStringSlot {
    uint256[32] private unused;
    string private value;

    function lengthThroughRef() external returns (uint256) {
        value = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        return StorageStringRef.length(value);
    }

    function returnThroughRef() external returns (uint256) {
        value = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        return bytes(StorageStringRef.read(value)).length;
    }
}
