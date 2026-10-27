//@ codegen-matrix: standard
//@ run-call: forwarded 0, 0xabababababababababababababababababababababababababababababababab => 0x4f5096a1a309957fb165bfafea525a2fb90d53a1ead741effbfe9ce33130b737, 0, 0
//@ run-call: forwarded 5, 0xabababababababababababababababababababababababababababababababab => 0xe9ef173d7f2fe4518ac17868dc7aada59d8c6ee37359c774127ac09ce2176834, 5, 5
//@ run-call: forwarded 1606938044258990275541962092341162602522202993782792835301383, 0xabababababababababababababababababababababababababababababababab => 0x55f79190823f7dd887c9306d3d92715e018e7ecda8670180194bfb8594bd9713, 7, 7
//@ run-call: reset 5, 0xabababababababababababababababababababababababababababababababab => 0xe9ef173d7f2fe4518ac17868dc7aada59d8c6ee37359c774127ac09ce2176834, 0x0000000000000000000000000000000000000000000000000000000000000005
//@ run-call: branch true, 5, 0xabababababababababababababababababababababababababababababababab => 0x6852be1837581baaf36e140263e62f4b2bb055575eaffa00bdce4feefbfd43b2, 5
//@ run-call: branch false, 5, 0xabababababababababababababababababababababababababababababababab => 0x945b7120b9f25a4deebd7693498c6668ee3d4b2bcf9978ead2976c6d7d17ea8f, 0
//@ run-call: loop 0, 0xabababababababababababababababababababababababababababababababab => 0xabababababababababababababababababababababababababababababababab, 0
//@ run-call: loop 1, 0xabababababababababababababababababababababababababababababababab => 0xc69a3de1ba9dd2965a8a2a7a5f85f30ea0dbb7b3c894ca3afda4badf485ce003, 1
//@ run-call: loop 3, 0xabababababababababababababababababababababababababababababababab => 0x18e158bb9d802d43ee5f38383223241aa1e38115240a375fd51b760e2bce21a1, 6

contract FreshFmpAlias {
    struct Deposit {
        address sender;
        uint128 amount;
        bytes32 key;
    }

    // The encoding buffer starts at the free-memory pointer, past the struct.
    function forwarded(uint256 x, bytes32 prev)
        external
        pure
        returns (bytes32 digest, uint256 amount, uint256 sender)
    {
        Deposit memory d = Deposit({sender: address(uint160(x)), amount: uint128(x), key: prev});
        digest = keccak256(abi.encode(uint8(1), d, prev));
        amount = d.amount;
        sender = uint256(uint160(d.sender));
    }

    // Moving the pointer back to `e` puts the encoding on top of it, after the stores that
    // follow the move. Whether the encoding starts with a length word differs between
    // compilers; for `x < 2 ** 128` the third word from the buffer start or the second after
    // a length word is `x` either way, and it lands on `e.key`.
    function reset(uint256 x, bytes32 prev) external pure returns (bytes32 digest, bytes32 key) {
        Deposit memory d = Deposit({sender: address(uint160(x)), amount: uint128(x), key: prev});
        Deposit memory e = Deposit({sender: address(1), amount: 2, key: prev});
        assembly {
            mstore(0x40, e)
        }
        e.sender = address(uint160(x));
        e.amount = uint128(x);
        e.key = prev;
        digest = keccak256(abi.encode(uint8(1), d, prev));
        key = e.key;
    }

    // The struct exists on one branch only; the last encoding follows the join.
    function branch(bool take, uint256 x, bytes32 prev)
        external
        pure
        returns (bytes32 digest, uint256 amount)
    {
        if (take) {
            Deposit memory d =
                Deposit({sender: address(uint160(x)), amount: uint128(x), key: prev});
            digest = keccak256(abi.encode(uint8(1), d, prev));
            amount = d.amount;
        } else {
            digest = keccak256(abi.encode(uint8(2), prev));
        }
        digest = keccak256(abi.encode(digest, prev));
    }

    // Each iteration allocates a new struct.
    function loop(uint256 n, bytes32 prev) external pure returns (bytes32 acc, uint256 total) {
        acc = prev;
        for (uint256 i; i < n; ++i) {
            Deposit memory d =
                Deposit({sender: address(uint160(i + 1)), amount: uint128(i + 1), key: acc});
            acc = keccak256(abi.encode(uint8(1), d, acc));
            total += d.amount;
        }
    }
}
