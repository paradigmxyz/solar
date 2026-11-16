//@ codegen-matrix: standard
//@ run-call: r => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
// The constructor's spill area lies above its staging area and not above the fixed memory ranges
// it names, so a write to one keeps the values live across it out of the spill area: this
// constructor holds twenty-four values across a copy over low memory.
contract ConstructorConstantWriteSpills {
    bytes32 public r;

    constructor() payable {
        uint256 a = msg.value + 5;
        bytes32 h0 = keccak256(abi.encodePacked(a, uint256(0)));
        bytes32 h1 = keccak256(abi.encodePacked(a, uint256(1)));
        bytes32 h2 = keccak256(abi.encodePacked(a, uint256(2)));
        bytes32 h3 = keccak256(abi.encodePacked(a, uint256(3)));
        bytes32 h4 = keccak256(abi.encodePacked(a, uint256(4)));
        bytes32 h5 = keccak256(abi.encodePacked(a, uint256(5)));
        bytes32 h6 = keccak256(abi.encodePacked(a, uint256(6)));
        bytes32 h7 = keccak256(abi.encodePacked(a, uint256(7)));
        bytes32 h8 = keccak256(abi.encodePacked(a, uint256(8)));
        bytes32 h9 = keccak256(abi.encodePacked(a, uint256(9)));
        bytes32 h10 = keccak256(abi.encodePacked(a, uint256(10)));
        bytes32 h11 = keccak256(abi.encodePacked(a, uint256(11)));
        bytes32 h12 = keccak256(abi.encodePacked(a, uint256(12)));
        bytes32 h13 = keccak256(abi.encodePacked(a, uint256(13)));
        bytes32 h14 = keccak256(abi.encodePacked(a, uint256(14)));
        bytes32 h15 = keccak256(abi.encodePacked(a, uint256(15)));
        bytes32 h16 = keccak256(abi.encodePacked(a, uint256(16)));
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        assembly {
            calldatacopy(0x80, calldatasize(), 0x400)
        }
        r = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23;
    }
}
