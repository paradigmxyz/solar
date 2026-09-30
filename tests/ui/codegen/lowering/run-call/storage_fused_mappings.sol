//@ codegen-matrix: standard
//@ run-call: roundTrip 0x00000000000000000000000000000000000000aa => 1000, 7, 99
//@ run-call: rawRecord 0x00000000000000000000000000000000000000aa => 0x00000000000000630000000000000007000000000000000000000000000003e8, 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: independent 0x00000000000000000000000000000000000000aa, 0x00000000000000000000000000000000000000bb => 1, 2, 3, 10, 20, 30
//@ run-call: deleteOne 0x00000000000000000000000000000000000000aa => 1000, 0, 99
//@ run-call: compound 0x00000000000000000000000000000000000000aa => 340282366920938463463374607431768211455, 8
//@ run-call-fail: overflow 0x00000000000000000000000000000000000000aa => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: getters 0x00000000000000000000000000000000000000aa => 5, 6, 7
//@ run-call: orders 42 => 0x000000000000000000000000000000000000bEEF, true, 115792089237316195423570985008687907853269984665640564039457584007913129639935, -3, 0x000000000000000000000001000000000000000000000000000000000000beef, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0x00000000000000000000000000000000000000000000000000000000000000fd
//@ run-call: plainStaysStandard 0x00000000000000000000000000000000000000aa => 0x0000000000000000000000000000000000000000000000000000000000000009

// `@custom:solar-fuse` keeps the values a group of mappings holds for one key
// in one record at keccak256(key . slot of the group's first mapping), laid out
// like a struct's fields. Reads and writes behave as without the tag; raw reads
// of the record show the layout.
contract FusedMappings {
    /// @custom:solar-fuse account
    mapping(address => uint128) public balance;
    /// @custom:solar-fuse account
    mapping(address => uint64) public nonce;
    /// @custom:solar-fuse account
    mapping(address => uint64) public expiry;
    mapping(address => uint256) plain;

    // A word-sized value starts a new word of the record, and so does the value after it.
    /// @custom:solar-fuse order
    mapping(uint256 => address) owner;
    /// @custom:solar-fuse order
    mapping(uint256 => bool) open;
    /// @custom:solar-fuse order
    mapping(uint256 => uint256) amount;
    /// @custom:solar-fuse order
    mapping(uint256 => int8) side;

    function roundTrip(address a) external returns (uint128, uint64, uint64) {
        balance[a] = 1000;
        nonce[a] = 7;
        expiry[a] = 99;
        return (balance[a], nonce[a], expiry[a]);
    }

    // One word holds the account; `nonce`'s standard slot for the key stays empty.
    function rawRecord(address a) external returns (bytes32 word, bytes32 next, bytes32 standard) {
        balance[a] = 1000;
        nonce[a] = 7;
        expiry[a] = 99;
        bytes32 record = keccak256(abi.encode(a, uint256(0)));
        bytes32 nonceSlot = keccak256(abi.encode(a, uint256(1)));
        assembly {
            word := sload(record)
            next := sload(add(record, 1))
            standard := sload(nonceSlot)
        }
    }

    function independent(address a, address b)
        external
        returns (uint128, uint64, uint64, uint128, uint64, uint64)
    {
        balance[a] = 1;
        nonce[a] = 2;
        expiry[a] = 3;
        balance[b] = 10;
        nonce[b] = 20;
        expiry[b] = 30;
        return (balance[a], nonce[a], expiry[a], balance[b], nonce[b], expiry[b]);
    }

    function deleteOne(address a) external returns (uint128, uint64, uint64) {
        balance[a] = 1000;
        nonce[a] = 7;
        expiry[a] = 99;
        delete nonce[a];
        return (balance[a], nonce[a], expiry[a]);
    }

    function compound(address a) external returns (uint128, uint64) {
        balance[a] = type(uint128).max - 1;
        balance[a] += 1;
        nonce[a] = 7;
        nonce[a]++;
        return (balance[a], nonce[a]);
    }

    // Checked arithmetic on a fused value fails as on any other.
    function overflow(address a) external returns (uint128) {
        balance[a] = type(uint128).max;
        balance[a] += 1;
        return balance[a];
    }

    function getters(address a) external returns (uint128, uint64, uint64) {
        balance[a] = 5;
        nonce[a] = 6;
        expiry[a] = 7;
        return (this.balance(a), this.nonce(a), this.expiry(a));
    }

    function orders(uint256 id)
        external
        returns (address, bool, uint256, int8, bytes32 w0, bytes32 w1, bytes32 w2)
    {
        owner[id] = address(0xbeef);
        open[id] = true;
        amount[id] = type(uint256).max;
        side[id] = -3;
        bytes32 record = keccak256(abi.encode(id, uint256(4)));
        assembly {
            w0 := sload(record)
            w1 := sload(add(record, 1))
            w2 := sload(add(record, 2))
        }
        return (owner[id], open[id], amount[id], side[id], w0, w1, w2);
    }

    // An untagged mapping keeps the standard layout.
    function plainStaysStandard(address a) external returns (bytes32 word) {
        plain[a] = 9;
        bytes32 slot = keccak256(abi.encode(a, uint256(3)));
        assembly {
            word := sload(slot)
        }
    }
}
