//@ codegen-matrix: standard
//@ run-call: reuse 0 => 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: reuse 3 => 0xe37710aff3716aa127b8a1e833829ba3b4c07816ec1c4443944b45b45b8adc24, 0x6cce308dccb881a18797ed30f4eddbe5686a4a287f818b889590817dce28dabe
//@ run-call: dirty 9 => 0x900c23a9d46ff3fba1a84e11dc04522c56a40fbd93ed0aecb723b410a38a3243, 0x0000000000000000000000000000000000000000000000000000000000000000, 96
//@ run-call: fill 0 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: fill 4 => 0x2a2c1a9f9c9fbebbf2dce2f8e9b96d5bcf0374b203bb2f7330def1cdbc47eb8c
//@ run-call: lengthFirst 5 => 32, 0x036b6384b5eca791c62761152d0c79bb0604c104a5fb6f4eb0703f3154bb3db0

// A zeroed allocation that is written in full before any read skips its
// zeroing. Memory past the free memory pointer is not zero after a scratch
// block or a staged encoding, so every result here matches solc's.
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Test {
    using Bytes for bytes;

    // Each iteration reuses the memory the previous blocks wrote. A buffer
    // written in full needs no zeroing; one written in part still reads zeros
    // where it was not written.
    function reuse(uint256 n) public pure returns (bytes32 full, bytes32 halves) {
        for (uint256 i = 1; i <= n; ++i) {
            /// @custom:solar-scratch
            {
                bytes memory whole = new bytes(64);
                whole.writeUint256BE(0, i);
                whole.writeUint256BE(32, i * 7);
                full = keccak256(abi.encodePacked(full, keccak256(whole)));
            }
            /// @custom:solar-scratch
            {
                bytes memory half = new bytes(64);
                half.writeUint256BE(0, i);
                halves = keccak256(abi.encodePacked(halves, keccak256(half)));
            }
        }
    }

    // Hashing an encoding stages it past the free memory pointer, so the next
    // allocation starts in memory that is not zero.
    function dirty(uint256 x) public pure returns (bytes32 h, bytes32 tail, uint256 length) {
        h = keccak256(abi.encode(x, x, x, x, x));
        bytes memory buf = new bytes(96);
        buf.writeUint256BE(0, x);
        tail = buf.readBytes32(32);
        length = buf.length;
    }

    function fill(uint256 n) public pure returns (bytes32 acc) {
        for (uint256 i; i < n; ++i) {
            bytes memory b = new bytes(32);
            b.writeUint256BE(0, i + 1);
            acc = keccak256(abi.encodePacked(acc, b));
        }
    }

    // A read of the length before the data is written reads what was stored.
    function lengthFirst(uint256 x) public pure returns (uint256 length, bytes32 h) {
        bytes memory b = new bytes(32);
        length = b.length;
        b.writeUint256BE(0, x);
        h = keccak256(b);
    }
}
