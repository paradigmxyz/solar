//@ codegen-matrix: standard
//@ compile-flags: --allow=0067
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: literal(uint256) 100 => 0, true
//@ run-call: literal(uint256) 400 => 0, true
//@ run-call: constantInput(uint256) 100 => 9600
//@ run-call: variants(uint256) 100 => 19200
//@ run-call: argumentOrder => 2, 0
//@ run-call: largeLiteral(uint256) 100 => 0

contract LiteralTarget {
    function hash(bytes calldata reason) external pure returns (bytes32) {
        return keccak256(reason);
    }

    function check(uint256 a, uint256 b, string calldata reason) external pure {
        require(a < b);
        require(keccak256(bytes(reason)) == keccak256("assertion failed: balance must stay below the limit!!"));
    }
}

contract LiteralInputMemory {
    LiteralTarget target = new LiteralTarget();
    string constant REASON = "assertion failed: balance must stay below the limit!!";
    string constant LONG = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdef";
    uint256 counter;

    function tick() internal returns (uint256) { return ++counter; }

    function argumentOrder() external returns (uint256 count, uint256 growth) {
        uint256 beforePointer;
        assembly { beforePointer := mload(64) }
        target.check({reason: "assertion failed: balance must stay below the limit!!", a: tick(), b: tick()});
        assembly { growth := sub(mload(64), beforePointer) }
        count = counter;
    }

    function variants(uint256 n) external view returns (uint256 growth) {
        function(bytes memory) external view returns (bytes32) callback = target.hash;
        uint256 beforePointer;
        assembly { beforePointer := mload(64) }
        for (uint256 k; k < n; ++k) {
            require(callback(bytes(LONG)) == keccak256(bytes(LONG)));
            require(target.hash(hex"00ff00") == keccak256(hex"00ff00"));
            require(target.hash("") == keccak256(""));
            target.check({reason: "assertion failed: balance must stay below the limit!!", b: k + 1, a: k});
        }
        assembly { growth := sub(mload(64), beforePointer) }
    }

    function literal(uint256 n) external view returns (uint256 growth, bool bounded) {
        uint256 beforePointer;
        uint256 beforeSize;
        assembly { beforePointer := mload(64) beforeSize := msize() }
        for (uint256 k; k < n; ++k) {
            target.check(k, k + 1, "assertion failed: balance must stay below the limit!!");
        }
        uint256 size;
        assembly { growth := sub(mload(64), beforePointer) size := sub(msize(), beforeSize) }
        bounded = size < 1024;
    }

    function constantInput(uint256 n) external view returns (uint256 growth) {
        uint256 beforePointer;
        assembly { beforePointer := mload(64) }
        for (uint256 k; k < n; ++k) target.check(k, k + 1, REASON);
        assembly { growth := sub(mload(64), beforePointer) }
    }

    function largeLiteral(uint256 n) external view returns (uint256 growth) {
        uint256 beforePointer;
        assembly { beforePointer := mload(64) }
        for (uint256 k; k < n; ++k) {
            require(target.hash(hex"ff4c675011b45284bfd6c5fe2bcc1d645a7ce02f3d932034fcfa1834fcbb207d6648e8ad98e14bce62074500c8eacb708a9d17131f56a7ad493223e07b48eee669bd83d08087875638c4dcfae97ab07797a42d0973c2e50163d9b04c237ea11661ea37bb3c209619554ba1f5b2d0b2828234fe7b0a76da0c366c4985c63501d13f62fabe2aa2579715116c491e06b707c1366a7553f90cd8066841f4772a34d456b2417df24f1a3ce47883b08aae2a8692d2c509483bebfc0cb56b41b3f6911253e95d2ad8588b39e2d8dbe2642ac81a3cef790eb832bcde2ae7bb95e8479bcc5f3c7d4a7311f7f29e4f4064c73437d9c9173f4473cb183f14136f8d3b4bbea3ff") == keccak256(hex"ff4c675011b45284bfd6c5fe2bcc1d645a7ce02f3d932034fcfa1834fcbb207d6648e8ad98e14bce62074500c8eacb708a9d17131f56a7ad493223e07b48eee669bd83d08087875638c4dcfae97ab07797a42d0973c2e50163d9b04c237ea11661ea37bb3c209619554ba1f5b2d0b2828234fe7b0a76da0c366c4985c63501d13f62fabe2aa2579715116c491e06b707c1366a7553f90cd8066841f4772a34d456b2417df24f1a3ce47883b08aae2a8692d2c509483bebfc0cb56b41b3f6911253e95d2ad8588b39e2d8dbe2642ac81a3cef790eb832bcde2ae7bb95e8479bcc5f3c7d4a7311f7f29e4f4064c73437d9c9173f4473cb183f14136f8d3b4bbea3ff"));
        }
        assembly { growth := sub(mload(64), beforePointer) }
    }
}
