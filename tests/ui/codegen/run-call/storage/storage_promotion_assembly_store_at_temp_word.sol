//@ codegen-matrix: standard
//@ run-call: tail 3 => 8
//@ run-call: body 3 => 8
//@ run-call: pair 3 => 16
//@ run-call: copy 3 => 8
//@ run-call: earlier 3 => 4369
//@ run-call: callee 3 => 4369
//@ run-call: publicCallee 3 => 4369
//@ run-call: caller 3 => 4369

// Assembly that names the address of a promoted slot's memory temporary, before its initial store
// or between that store and the flush, must neither reach the temporary nor lose its word to it.
contract StoragePromotionAssemblyStoreAtTempWord {
    uint256 public x;
    uint256 public y;

    function tail(uint256 n) external returns (uint256) {
        x = 5;
        assembly { mstore(0x80, 0x1111) }
        for (uint256 i; i < n; ++i) x += 1;
        return x;
    }

    function body(uint256 n) external returns (uint256) {
        x = 5;
        for (uint256 i; i < n; ++i) {
            x += 1;
            assembly { mstore(0x80, 0x1111) }
        }
        return x;
    }

    function copy(uint256 n) external returns (uint256) {
        x = 5;
        assembly { calldatacopy(0x80, 0, calldatasize()) }
        for (uint256 i; i < n; ++i) x += 1;
        return x;
    }

    // The assembly's word at the temporary's address is read after the loop.
    function earlier(uint256 n) external returns (uint256 r) {
        assembly { mstore(0x80, 0x1111) }
        x = 5;
        for (uint256 i; i < n; ++i) x = x * 2 + 1;
        assembly {
            pop(staticcall(gas(), 4, 0x80, 0x20, 0x300, 0x20))
            r := mload(0x300)
        }
    }

    // A called function owns the word.
    function callee(uint256 n) external returns (uint256 r) {
        put();
        x = 5;
        for (uint256 i; i < n; ++i) x = x * 2 + 1;
        return get();
    }

    function put() internal pure {
        assembly { mstore(0x80, 0x1111) }
    }

    function get() internal pure returns (uint256 r) {
        assembly { r := mload(0x80) }
    }

    // A public function that the entry calls owns the word too.
    function publicCallee(uint256 n) external returns (uint256 r) {
        putPublic();
        x = 5;
        for (uint256 i; i < n; ++i) x = x * 2 + 1;
        return getPublic();
    }

    function putPublic() public pure {
        assembly {
            for { let i := 0 } lt(i, 3) { i := add(i, 1) } { mstore(0x80, 0x1111) }
        }
    }

    function getPublic() public pure returns (uint256 r) {
        assembly {
            for { let i := 0 } lt(i, 3) { i := add(i, 1) } { r := mload(0x80) }
        }
    }

    // A caller keeps its word at the temporary's address live across the call.
    function caller(uint256 n) external returns (uint256 r) {
        assembly { mstore(0x80, 0x1111) }
        bump(n);
        assembly { r := mload(0x80) }
    }

    function bump(uint256 n) public returns (uint256) {
        x = 5;
        for (uint256 i; i < n; ++i) x += 1;
        return x;
    }

    function pair(uint256 n) external returns (uint256) {
        x = 5;
        y = 7;
        assembly {
            mstore(0xa0, 0x2222)
            mstore(0xc0, 0x3333)
        }
        for (uint256 i; i < n; ++i) {
            x += 1;
            y += 1;
        }
        return x + y - 2;
    }
}
