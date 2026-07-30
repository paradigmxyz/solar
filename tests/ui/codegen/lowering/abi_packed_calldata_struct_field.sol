//@compile-flags: -Zcodegen --emit=bin-runtime
// ported-from: contracts/metatx/ERC2771Forwarder.sol

// Packing a dynamic field of a calldata struct. The prologue decodes the
// struct into memory, so the field is already the `[length][data...]`
// pointer this packs from even though its type is calldata-located; the
// calldata-slice path would have copied from the wrong place. The check
// runs before `calldata_bytes_source`, which would otherwise lower the
// expression a second time into whichever block is current.
struct ForwardRequestData {
    address from;
    address to;
    uint256 value;
    uint256 gas;
    uint48 deadline;
    bytes data;
    bytes signature;
}

contract AbiPackedCalldataStructField {
    function pack(ForwardRequestData calldata request) external pure returns (bytes memory) {
        return abi.encodePacked(request.data, request.from);
    }

    function packHash(ForwardRequestData calldata request) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(request.data, request.from));
    }
}
