// Solc test: test/libsolidity/syntaxTests/using/free_reference_type.sol.

//@compile-flags: -Ztypeck

function memoryHead(uint256[] memory x) pure returns (uint256) {
    return x[0];
}

function storageHead(uint256[] storage x) view returns (uint256) {
    return x[0];
}

function calldataHead(uint256[] calldata x) pure returns (uint256) {
    return x[0];
}

using {memoryHead, storageHead, calldataHead} for uint256[];

contract C {
    uint256[] s;

    function fromStorage() public view returns (uint256) {
        return s.storageHead();
    }

    function fromMemory(uint256[] memory xs) public pure returns (uint256) {
        return xs.memoryHead();
    }

    function fromCalldata(uint256[] calldata xs) public pure returns (uint256) {
        return xs.calldataHead();
    }
}
