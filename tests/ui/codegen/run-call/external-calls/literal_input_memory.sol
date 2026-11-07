//@ codegen-matrix: standard
//@ compile-flags: --allow=0067
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: literal(uint256) 100 => 0, true
//@ run-call: literal(uint256) 400 => 0, true
//@ run-call: constantInput(uint256) 100 => 0
//@ run-call: variants(uint256) 100 => 0
//@ run-call: argumentOrder => 2, 0

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
}
