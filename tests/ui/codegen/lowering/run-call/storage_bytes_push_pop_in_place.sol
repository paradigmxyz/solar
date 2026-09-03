//@ codegen-matrix: standard
//@[mir] filecheck: --implicit-check-not=store_storage_bytes
//@ run-call: pushRange 0 => 0x
//@ run-call: pushRange 1 => 0x01
//@ run-call: pushRange 30 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e
//@ run-call: pushRange 31 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
//@ run-call: pushRange 32 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
//@ run-call: pushRange 33 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021
//@ run-call: pushRange 64 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40
//@ run-call: pushRange 65 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f4041
//@ run-call: pushZero 1 => 0x00, 1
//@ run-call: pushZero 31 => 0x00000000000000000000000000000000000000000000000000000000000000, 31
//@ run-call: pushZero 32 => 0x0000000000000000000000000000000000000000000000000000000000000000, 32
//@ run-call: pushZero 33 => 0x000000000000000000000000000000000000000000000000000000000000000000, 33
//@ run-call: pushZero 64 => 0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000, 64
//@ run-call: pushAssign 1 => 0x01
//@ run-call: pushAssign 31 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
//@ run-call: pushAssign 32 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
//@ run-call: pushAssign 33 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021
//@ run-call: pushAssign 64 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40
//@ run-call: popTo 1, 1 => 0x, 0
//@ run-call: popTo 31, 1 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e, 30
//@ run-call: popTo 32, 1 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 31
//@ run-call: popTo 32, 2 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e, 30
//@ run-call: popTo 33, 1 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 32
//@ run-call: popTo 33, 2 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 31
//@ run-call: popTo 33, 33 => 0x, 0
//@ run-call: popTo 64, 32 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 32
//@ run-call: popTo 64, 33 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 31
//@ run-call: popTo 65, 34 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 31
//@ run-call: slots 0, 0 => 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: slots 31, 0 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f3e, 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: slots 31, 31 => 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: slots 32, 0 => 0x0000000000000000000000000000000000000000000000000000000000000041, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: slots 32, 1 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f3e, 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: slots 33, 0 => 0x0000000000000000000000000000000000000000000000000000000000000043, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 0x2100000000000000000000000000000000000000000000000000000000000000
//@ run-call: slots 33, 1 => 0x0000000000000000000000000000000000000000000000000000000000000041, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: slots 64, 32 => 0x0000000000000000000000000000000000000000000000000000000000000041, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: slots 64, 33 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f3e, 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: slots 1, 1 => 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: interleaved 1 => 1, 1, 1, 1
//@ run-call: interleaved 31 => 31, 1, 16, 31
//@ run-call: interleaved 32 => 32, 1, 17, 32
//@ run-call: interleaved 33 => 33, 1, 17, 33
//@ run-call: interleaved 64 => 64, 1, 33, 64
//@ run-call-fail: popEmptyAfter 0 => Panic(0x31)
//@ run-call-fail: popEmptyAfter 1 => Panic(0x31)
//@ run-call-fail: popEmptyAfter 31 => Panic(0x31)
//@ run-call-fail: popEmptyAfter 32 => Panic(0x31)
//@ run-call-fail: popEmptyAfter 33 => Panic(0x31)
//@ run-call-fail: popUntouched => Panic(0x31)
//@ run-call: structPushPop 1 => 0x, 1
//@ run-call: structPushPop 32 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 32
//@ run-call: structPushPop 33 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 33
//@ run-call: mappingPushPop 1 => 0x
//@ run-call: mappingPushPop 32 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
//@ run-call: mappingPushPop 33 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
//@ run-call: pointerPushPop 1 => 0x
//@ run-call: pointerPushPop 32 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
//@ run-call: pointerPushPop 33 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
//@ run-call: pushPopWords 2 => 1, 1
//@ run-call: pushPopWords 33 => 32, 32
//@ run-call: pushPopWords 34 => 33, 33
//@ run-call: pushZeroRead 0 => 0xff, 0xff00000000000000000000000000000000000000000000000000000000000002
//@ run-call: pushZeroRead 5 => 0xff, 0xffffffffffff000000000000000000000000000000000000000000000000000c
//@ run-call: pushZeroRead 30 => 0xff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff3e
//@ run-call: pushZeroRead 31 => 0x00, 0x0000000000000000000000000000000000000000000000000000000000000041
//@ run-call: pushZeroReadLong 0 => 0xff, 0x0000000000000000000000000000000000000000000000000000000000000043, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: pushZeroReadLong 31 => 0xff, 0x0000000000000000000000000000000000000000000000000000000000000081, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

// `push`, `push()`, and `pop` on a storage `bytes` or `string` update the value
// in place, so they follow Solidity's short/long layout: a short value keeps
// its data in the length slot, a long value keeps word `i / 32` in the array's
// data area, growing past 31 bytes moves the data out, and shrinking from 32
// bytes moves it back.
contract StorageBytesPushPop {
    struct S {
        uint256 head;
        bytes data;
    }

    bytes internal data;
    S internal s;
    mapping(uint256 => bytes) internal m;
    uint8[] internal words;

    function _push(uint256 n) internal {
        for (uint256 i; i < n; i++) {
            data.push(bytes1(uint8(i + 1)));
        }
    }

    // The push reads the length slot and writes the appended byte in place,
    // without copying the value through memory.
    // CHECK-LABEL: fn @pushOne{{[( ]}}
    // CHECK: sload 0
    // CHECK-NOT: internal_call
    // CHECK: sstore
    function pushOne(bytes1 value) external {
        data.push(value);
    }

    // CHECK-LABEL: fn @popOne{{[( ]}}
    // CHECK: sload 0
    // CHECK-NOT: internal_call
    // CHECK: sstore
    function popOne() external {
        _push(1);
        data.pop();
    }

    function pushRange(uint256 n) external returns (bytes memory) {
        _push(n);
        return data;
    }

    function pushZero(uint256 n) external returns (bytes memory, uint256) {
        for (uint256 i; i < n; i++) {
            data.push();
        }
        return (data, data.length);
    }

    function pushAssign(uint256 n) external returns (bytes memory) {
        for (uint256 i; i < n; i++) {
            data.push() = bytes1(uint8(i + 1));
        }
        return data;
    }

    function popTo(uint256 n, uint256 k) external returns (bytes memory, uint256) {
        _push(n);
        for (uint256 i; i < k; i++) {
            data.pop();
        }
        return (data, data.length);
    }

    // The header slot and the first two data words pin the encoding after the
    // short-to-long and long-to-short transitions, including the clearing of
    // popped bytes.
    function slots(uint256 n, uint256 k)
        external
        returns (bytes32 header, bytes32 first, bytes32 second)
    {
        _push(n);
        for (uint256 i; i < k; i++) {
            data.pop();
        }
        assembly {
            let slot := data.slot
            header := sload(slot)
            mstore(0, slot)
            let start := keccak256(0, 32)
            first := sload(start)
            second := sload(add(start, 1))
        }
    }

    function interleaved(uint256 n)
        external
        returns (uint256 length, uint256 first, uint256 middle, uint256 last)
    {
        for (uint256 i; i < n; i++) {
            data.push(bytes1(uint8(i + 1)));
            require(data.length == i + 1);
            require(uint8(data[i]) == i + 1);
        }
        length = data.length;
        first = uint8(data[0]);
        middle = uint8(data[n / 2]);
        last = uint8(data[n - 1]);
    }

    function popEmptyAfter(uint256 n) external {
        _push(n);
        for (uint256 i; i < n; i++) {
            data.pop();
        }
        require(data.length == 0);
        data.pop();
    }

    function popUntouched() external {
        data.pop();
    }

    function structPushPop(uint256 n) external returns (bytes memory, uint256) {
        s.head = n;
        for (uint256 i; i < n; i++) {
            s.data.push(bytes1(uint8(i + 1)));
        }
        s.data.pop();
        return (s.data, s.head);
    }

    function mappingPushPop(uint256 n) external returns (bytes memory) {
        for (uint256 i; i < n; i++) {
            m[7].push(bytes1(uint8(i + 1)));
        }
        m[7].pop();
        return m[7];
    }

    function pointerPushPop(uint256 n) external returns (bytes memory) {
        return _pushPop(data, n);
    }

    function _pushPop(bytes storage target, uint256 n) internal returns (bytes memory) {
        for (uint256 i; i < n; i++) {
            target.push(bytes1(uint8(i + 1)));
        }
        target.pop();
        return target;
    }

    // `push()` yields the appended element, and like solc it leaves whatever
    // that element already held; only inline assembly can leave it dirty, so
    // reading the call's value must load the byte instead of assuming zero.
    // The short cases also cover the 31-byte transition, whose appended byte is
    // the one the header rewrite clears.
    function pushZeroRead(uint256 len) external returns (bytes1 got, bytes32 header) {
        assembly {
            sstore(data.slot, or(not(0xff), mul(mod(len, 32), 2)))
        }
        got = data.push();
        assembly {
            header := sload(data.slot)
        }
    }

    function pushZeroReadLong(uint256 n)
        external
        returns (bytes1 got, bytes32 header, bytes32 word)
    {
        assembly {
            sstore(data.slot, add(mul(add(mod(n, 32), 32), 2), 1))
            mstore(0, data.slot)
            let start := keccak256(0, 32)
            sstore(start, not(0))
            sstore(add(start, 1), not(0))
            sstore(add(start, 2), not(0))
        }
        got = data.push();
        assembly {
            mstore(0, data.slot)
            header := sload(data.slot)
            word := sload(add(keccak256(0, 32), 1))
        }
    }

    // A packed `uint8[]` keeps the plain dynamic-array lowering.
    function pushPopWords(uint256 n) external returns (uint256, uint256) {
        for (uint256 i; i < n; i++) {
            words.push(uint8(i + 1));
        }
        words.pop();
        return (words.length, words[words.length - 1]);
    }
}
