//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: ret 33 => 0x414141414141414141414141414141414141414141414141414141414141414141
//@ run-call: ret_probe 33 => true
//@ run-call: ret 63 => 0x414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141
//@ run-call: ret_probe 63 => true
//@ run-call-fail: revert_custom 33 => E(bytes)(0x414141414141414141414141414141414141414141414141414141414141414141)
//@ run-call: revert_custom_probe 33 => true
//@ run-call-fail: revert_custom 63 => E(bytes)(0x414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141)
//@ run-call: revert_custom_probe 63 => true
//@ run-call-fail: revert_string 33 => Error("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
//@ run-call: revert_string_probe 33 => true
//@ run-call-fail: revert_string 63 => Error("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
//@ run-call: revert_string_probe 63 => true
//@ run-call: encode 33 => 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002141414141414141414141414141414141414141414141414141414141414141414100000000000000000000000000000000000000000000000000000000000000
//@ run-call: encode_probe 33 => true
//@ run-call: encode 63 => 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003f41414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414100
//@ run-call: encode_probe 63 => true
//@ run-call: packed 33 => 0x41414141414141414141414141414141414141414141414141414141414141414109
//@ run-call: packed_probe 33 => true
//@ run-call: packed 63 => 0x41414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414109
//@ run-call: packed_probe 63 => true
//@ run-call: encode_call 33 => 0xa49cb33b0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002141414141414141414141414141414141414141414141414141414141414141414100000000000000000000000000000000000000000000000000000000000000
//@ run-call: encode_call_probe 33 => true
//@ run-call: encode_call 63 => 0xa49cb33b0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003f41414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414100
//@ run-call: encode_call_probe 63 => true
//@ run-call: concat 33 => 0x41414141414141414141414141414141414141414141414141414141414141414101
//@ run-call: concat_probe 33 => true
//@ run-call: concat 63 => 0x41414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414101
//@ run-call: concat_probe 63 => true
//@ run-call: concat_string 33 => "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAx"
//@ run-call: concat_string_probe 33 => true
//@ run-call: concat_string 63 => "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAx"
//@ run-call: concat_string_probe 63 => true
//@ run-call: to_bytes32 33 => 0x4141414141414141414141414141414141414141414141414141414141414141
//@ run-call: to_bytes32_probe 33 => true
//@ run-call: to_bytes32 63 => 0x4141414141414141414141414141414141414141414141414141414141414141
//@ run-call: to_bytes32_probe 63 => true
//@ run-call: to_bytes4 33 => 0x41414141
//@ run-call: to_bytes4_probe 33 => true
//@ run-call: to_bytes4 63 => 0x41414141
//@ run-call: to_bytes4_probe 63 => true
//@ run-call: keccak 33 => 0x1dbcddc7b1d94f2e58c2ad2a5f2625136eddf1141cbd67a640ef62ba832bc87c
//@ run-call: keccak_probe 33 => true
//@ run-call: keccak 63 => 0x01b735fa8b3f08d6ce22339bc8cda3dd40c7a6268dcb2d0707fc2c4d1c4beb2b
//@ run-call: keccak_probe 63 => true
//@ run-call: last_byte 33 => 0x41
//@ run-call: last_byte_probe 33 => true
//@ run-call: last_byte 63 => 0x41
//@ run-call: last_byte_probe 63 => true
//@ run-call: push 33 => 0x41414141414141414141414141414141414141414141414141414141414141414101
//@ run-call: push_probe 33 => true
//@ run-call: push 63 => 0x41414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414101
//@ run-call: push_probe 63 => true
//@ run-call: store_raw 33 => 0x0000000000000000000000000000000000000000000000000000000000000043, 0x4141414141414141414141414141414141414141414141414141414141414141, 0x4100000000000000000000000000000000000000000000000000000000000000
//@ run-call: store_raw_probe 33 => true
//@ run-call: store_raw 63 => 0x000000000000000000000000000000000000000000000000000000000000007f, 0x4141414141414141414141414141414141414141414141414141414141414141, 0x4141414141414141414141414141414141414141414141414141414141414100
//@ run-call: store_raw_probe 63 => true
//@ run-call: forward 33 => 0x624fbfdc0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002141414141414141414141414141414141414141414141414141414141414141414100000000000000000000000000000000000000000000000000000000000000
//@ run-call: forward_probe 33 => true
//@ run-call: forward 63 => 0x624fbfdc0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003f41414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414100
//@ run-call: forward_probe 63 => true
//@ run-call: 0x7db29baf0000000000000000000000000000000000000000000000000000000000000021 => 0x414141414141414141414141414141414141414141414141414141414141414141ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffdb
//@ run-call: asm_return_probe 33 => true
//@ run-call: 0x7db29baf000000000000000000000000000000000000000000000000000000000000003f => 0x414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141db
//@ run-call: asm_return_probe 63 => true
//@ run-call-fail: 0x6f0887f50000000000000000000000000000000000000000000000000000000000000021 => 0x414141414141414141414141414141414141414141414141414141414141414141ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffdb
//@ run-call: asm_revert_probe 33 => true
//@ run-call-fail: 0x6f0887f5000000000000000000000000000000000000000000000000000000000000003f => 0x414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141db
//@ run-call: asm_revert_probe 63 => true
//@ run-call: 0x33cfe53a0000000000000000000000000000000000000000000000000000000000000021 => 0x58a6229eaaa35e80a0433513f41b2f889652ea595a40a3b03d74dceb3d6e8d2c
//@ run-call: asm_keccak_probe 33 => true
//@ run-call: 0x33cfe53a000000000000000000000000000000000000000000000000000000000000003f => 0x856214e23057d61426996f739e36525f1fc8eb135337415f6d973f03f6dafa9b
//@ run-call: asm_keccak_probe 63 => true
//@ run-call: 0x13003d400000000000000000000000000000000000000000000000000000000000000021 => 0x414141414141414141414141414141414141414141414141414141414141414141ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffdb
//@ run-call: asm_staticcall_probe 33 => true
//@ run-call: 0x13003d40000000000000000000000000000000000000000000000000000000000000003f => 0x414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141db
//@ run-call: asm_staticcall_probe 63 => true
//@ run-call: 0x9352b00b0000000000000000000000000000000000000000000000000000000000000021 => 0x414141414141414141414141414141414141414141414141414141414141414141ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffdb
//@ run-call: asm_call_probe 33 => true
//@ run-call: 0x9352b00b000000000000000000000000000000000000000000000000000000000000003f => 0x414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141db
//@ run-call: asm_call_probe 63 => true
//@ run-call: 0x5e73c9870000000000000000000000000000000000000000000000000000000000000021 => 0x414141414141414141414141414141414141414141414141414141414141414141ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffdb
//@ run-call: asm_mcopy_probe 33 => true
//@ run-call: 0x5e73c987000000000000000000000000000000000000000000000000000000000000003f => 0x414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141db
//@ run-call: asm_mcopy_probe 63 => true

// The padding after a `bytes` object's last byte is not cleared when the object is `bytes.concat` into free memory that earlier operations dirtied.
// Every case below consumes the object with Solidity or with assembly that reads the padding, and
// the matching `_probe` function reads the padding word with assembly to show that it is nonzero
// at the point of consumption.
contract BytesPadding_concat {
    error E(bytes data);
    event Data(bytes data);
    event Topic(bytes indexed data);

    bytes private store;

    function sink(bytes memory) external pure {}

    function echo(bytes memory) external pure returns (bytes memory) {
        return msg.data;
    }

    // Returns `n` data bytes of 0x11 followed by 0xab padding.
    function dirty(uint256 n) external pure returns (bytes memory) {
        assembly {
            let rl := and(add(n, 31), not(31))
            mstore(0x00, 0x20)
            mstore(0x20, n)
            for { let i := 0 } lt(i, rl) { i := add(i, 32) } {
                mstore(add(0x40, i), 0x4141414141414141414141414141414141414141414141414141414141414141)
            }
            mstore(add(0x40, n), 0xabababababababababababababababababababababababababababababababab)
            return(0x00, add(0x40, rl))
        }
    }

    // Reverts with `Error(string)` data of `n` string bytes followed by 0xab padding.
    function dirtyRevert(uint256 n) external pure {
        assembly {
            let rl := and(add(n, 31), not(31))
            mstore(0x00, shl(224, 0x08c379a0))
            mstore(0x04, 0x20)
            mstore(0x24, n)
            for { let i := 0 } lt(i, rl) { i := add(i, 32) } {
                mstore(add(0x44, i), 0x4141414141414141414141414141414141414141414141414141414141414141)
            }
            mstore(add(0x44, n), 0xabababababababababababababababababababababababababababababababab)
            revert(0x00, add(0x44, rl))
        }
    }

    function _stir() private pure returns (bytes32) {
        uint256 m = ~msg.data.length;
        return keccak256(abi.encode(m, m, m, m, m, m, m, m, m, m, m, m, m, m, m, m));
    }

    function _packed(uint256 n) private pure returns (bytes memory) {
        bytes memory data = new bytes(n - 1);
        for (uint256 i; i < data.length; ++i) data[i] = bytes1(uint8(0x41));
        return abi.encodePacked(uint8(0x41), data);
    }

    function _concat(uint256 n) private pure returns (bytes memory) {
        bytes memory data = new bytes(n - 1);
        for (uint256 i; i < data.length; ++i) data[i] = bytes1(uint8(0x41));
        return bytes.concat("A", data);
    }

    function _fresh(uint256 n) private pure returns (bytes memory b) {
        b = new bytes(n);
        for (uint256 i; i < n; ++i) b[i] = bytes1(uint8(0x41));
    }

    // Whether the padding bytes after the end of `b` are nonzero.
    function _dirty(bytes memory b) private pure returns (bool d) {
        assembly {
            let l := mload(b)
            let r := and(l, 31)
            if r {
                d := iszero(iszero(shl(mul(8, r), mload(add(add(b, 32), and(l, not(31)))))))
            }
        }
    }

    function ret(uint256 n) external pure returns (bytes memory) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        return b;
    }

    function ret_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function revert_custom(uint256 n) external pure  {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        revert E(b);
    }

    function revert_custom_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function revert_string(uint256 n) external pure  {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        revert(string(b));
    }

    function revert_string_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function encode(uint256 n) external pure returns (bytes memory) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        return abi.encode(b);
    }

    function encode_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function packed(uint256 n) external pure returns (bytes memory) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        return abi.encodePacked(b, uint8(9));
    }

    function packed_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function encode_call(uint256 n) external view returns (bytes memory) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        return abi.encodeCall(this.sink, (b));
    }

    function encode_call_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function concat(uint256 n) external pure returns (bytes memory) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        return bytes.concat(b, hex"01");
    }

    function concat_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function concat_string(uint256 n) external pure returns (string memory) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        return string.concat(string(b), "x");
    }

    function concat_string_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function to_bytes32(uint256 n) external pure returns (bytes32) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        return bytes32(b);
    }

    function to_bytes32_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function to_bytes4(uint256 n) external pure returns (bytes4) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        return bytes4(b);
    }

    function to_bytes4_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function keccak(uint256 n) external pure returns (bytes32) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        return keccak256(b);
    }

    function keccak_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function last_byte(uint256 n) external pure returns (bytes1) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        return b[b.length - 1];
    }

    function last_byte_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function push(uint256 n) external returns (bytes memory) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        store = b;
        store.push(0x01);
        return store;
    }

    function push_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function store_raw(uint256 n) external returns (bytes32, bytes32, bytes32) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        store = b;
        bytes32 s0;
        bytes32 d0;
        bytes32 d1;
        assembly {
            s0 := sload(store.slot)
            mstore(0, store.slot)
            let p := keccak256(0, 0x20)
            d0 := sload(p)
            d1 := sload(add(p, 1))
        }
        return (s0, d0, d1);
    }

    function store_raw_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function forward(uint256 n) external view returns (bytes memory) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        return this.echo(b);
    }

    function forward_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function asm_return(uint256 n) external pure  {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        assembly {
            let rl := and(add(mload(b), 31), not(31))
            return(add(b, 32), rl)
        }
    }

    function asm_return_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function asm_revert(uint256 n) external pure  {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        assembly {
            let rl := and(add(mload(b), 31), not(31))
            revert(add(b, 32), rl)
        }
    }

    function asm_revert_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function asm_keccak(uint256 n) external pure returns (bytes32 h) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        assembly {
            let rl := and(add(mload(b), 31), not(31))
            h := keccak256(add(b, 32), rl)
        }
    }

    function asm_keccak_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function asm_staticcall(uint256 n) external view  {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        assembly {
            let rl := and(add(mload(b), 31), not(31))
            let out := mload(0x40)
            pop(staticcall(gas(), 4, add(b, 32), rl, out, rl))
            return(out, rl)
        }
    }

    function asm_staticcall_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function asm_call(uint256 n) external  {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        assembly {
            let rl := and(add(mload(b), 31), not(31))
            let out := mload(0x40)
            pop(call(gas(), 4, 0, add(b, 32), rl, out, rl))
            return(out, rl)
        }
    }

    function asm_call_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }

    function asm_mcopy(uint256 n) external pure  {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        assembly {
            let rl := and(add(mload(b), 31), not(31))
            let out := mload(0x40)
            mcopy(out, add(b, 32), rl)
            return(out, rl)
        }
    }

    function asm_mcopy_probe(uint256 n) external pure returns (bool d) {
        require(_stir() != bytes32(0));
        bytes memory b = _concat(n);
        d = _dirty(b);
    }
}
