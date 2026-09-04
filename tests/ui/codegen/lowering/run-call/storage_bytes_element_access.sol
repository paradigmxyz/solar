//@ codegen-matrix: standard
//@[mir] filecheck: --implicit-check-not=load_storage_bytes
//@ run-call: readSingle => 1, 1
//@ run-call: readEdges 31 => 1, 2, 30, 31, 31
//@ run-call: readEdges 32 => 1, 2, 31, 32, 32
//@ run-call: readEdges 33 => 1, 2, 32, 33, 33
//@ run-call: readEdges 100 => 1, 2, 99, 100, 100
//@ run-call: writeEdges 31 => 170, 2, 30, 187, 31
//@ run-call: writeEdges 32 => 170, 2, 31, 187, 32
//@ run-call: writeEdges 33 => 170, 2, 32, 187, 33
//@ run-call: writeEdges 100 => 170, 2, 99, 187, 100
//@ run-call: bumpAll 1 => 2, 1
//@ run-call: bumpAll 31 => 527, 31
//@ run-call: bumpAll 32 => 560, 32
//@ run-call: bumpAll 33 => 594, 33
//@ run-call: bumpAll 100 => 5150, 100
//@ run-call: compoundEdges 1 => 1, 1, 1
//@ run-call: compoundEdges 31 => 241, 15, 31
//@ run-call: compoundEdges 32 => 241, 0, 32
//@ run-call: compoundEdges 33 => 241, 1, 33
//@ run-call: compoundEdges 100 => 241, 4, 100
//@ run-call: deleteLast 31 => 0, 30, 31
//@ run-call: deleteLast 32 => 0, 31, 32
//@ run-call: deleteLast 33 => 0, 32, 33
//@ run-call: deleteLast 100 => 0, 99, 100
//@ run-call: rawByte 1 => 0x0100000000000000000000000000000000000000000000000000000000000000
//@ run-call: rawByte 31 => 0x1f00000000000000000000000000000000000000000000000000000000000000
//@ run-call: rawByte 32 => 0x2000000000000000000000000000000000000000000000000000000000000000
//@ run-call: rawByte 33 => 0x2100000000000000000000000000000000000000000000000000000000000000
//@ run-call: rawByte 100 => 0x6400000000000000000000000000000000000000000000000000000000000000
//@ run-call: shortString => 104, 111, 5
//@ run-call: longString => 97, 54, 33
//@ run-call: writeShortString => 90, 111, 5
//@ run-call: writeLongString => 90, 54, 33
//@ run-call-fail: at 0 => Panic(0x32)
//@ run-call-fail: readOob 0 => Panic(0x32)
//@ run-call-fail: readOob 1 => Panic(0x32)
//@ run-call-fail: readOob 31 => Panic(0x32)
//@ run-call-fail: readOob 32 => Panic(0x32)
//@ run-call-fail: readOob 33 => Panic(0x32)
//@ run-call-fail: writeOob 0 => Panic(0x32)
//@ run-call-fail: writeOob 31 => Panic(0x32)
//@ run-call-fail: writeOob 32 => Panic(0x32)
//@ run-call-fail: writeOob 33 => Panic(0x32)
//@ run-call-fail: deleteOob 32 => Panic(0x32)

// Indexing a storage `bytes` or `string` reads and writes the single storage
// word that holds the element, so it must handle both the short encoding, which
// keeps the data in the length slot, and the long encoding, which keeps word
// `i / 32` in the array's data area.
contract StorageBytesElementAccess {
    bytes internal data;
    string internal text;

    function _fill(uint256 n) internal {
        bytes memory value = new bytes(n);
        for (uint256 i; i < n; i++) {
            value[i] = bytes1(uint8(i + 1));
        }
        data = value;
    }

    // The element access reads the length slot, derives the data word, and
    // loads only that word.
    // CHECK-LABEL: fn @at{{[( ]}}
    // CHECK: sload 0
    // CHECK: storage_array_data_slot 0
    // CHECK: sload
    function at(uint256 i) external view returns (bytes1) {
        return data[i];
    }

    function readSingle() external returns (uint8 first, uint256 length) {
        _fill(1);
        first = uint8(data[0]);
        length = data.length;
    }

    function readEdges(uint256 n)
        external
        returns (uint8 first, uint8 second, uint8 secondLast, uint8 last, uint256 length)
    {
        _fill(n);
        first = uint8(data[0]);
        second = uint8(data[1]);
        secondLast = uint8(data[n - 2]);
        last = uint8(data[n - 1]);
        length = data.length;
    }

    function writeEdges(uint256 n)
        external
        returns (uint8 first, uint8 second, uint8 secondLast, uint8 last, uint256 length)
    {
        _fill(n);
        data[0] = 0xaa;
        data[n - 1] = 0xbb;
        first = uint8(data[0]);
        second = uint8(data[1]);
        secondLast = uint8(data[n - 2]);
        last = uint8(data[n - 1]);
        length = data.length;
    }

    function bumpAll(uint256 n) external returns (uint256 sum, uint256 length) {
        _fill(n);
        for (uint256 i; i < data.length; i++) {
            data[i] = bytes1(uint8(uint8(data[i]) + 1));
        }
        for (uint256 i; i < data.length; i++) {
            sum += uint8(data[i]);
        }
        length = data.length;
    }

    function compoundEdges(uint256 n) external returns (uint8 first, uint8 last, uint256 length) {
        _fill(n);
        data[0] |= 0xf0;
        data[n - 1] &= 0x0f;
        first = uint8(data[0]);
        last = uint8(data[n - 1]);
        length = data.length;
    }

    function deleteLast(uint256 n) external returns (uint8 zeroed, uint8 neighbour, uint256 length) {
        _fill(n);
        delete data[n - 1];
        zeroed = uint8(data[n - 1]);
        neighbour = uint8(data[n - 2]);
        length = data.length;
    }

    function rawByte(uint256 n) external returns (uint256 raw) {
        _fill(n);
        bytes1 last = data[n - 1];
        assembly {
            raw := last
        }
    }

    function shortString() external returns (uint8 first, uint8 last, uint256 length) {
        text = "hello";
        first = uint8(bytes(text)[0]);
        last = uint8(bytes(text)[4]);
        length = bytes(text).length;
    }

    function longString() external returns (uint8 first, uint8 last, uint256 length) {
        text = "abcdefghijklmnopqrstuvwxyz0123456";
        first = uint8(bytes(text)[0]);
        last = uint8(bytes(text)[32]);
        length = bytes(text).length;
    }

    function writeShortString() external returns (uint8 first, uint8 last, uint256 length) {
        text = "hello";
        bytes(text)[0] = "Z";
        first = uint8(bytes(text)[0]);
        last = uint8(bytes(text)[4]);
        length = bytes(text).length;
    }

    function writeLongString() external returns (uint8 first, uint8 last, uint256 length) {
        text = "abcdefghijklmnopqrstuvwxyz0123456";
        bytes(text)[0] = "Z";
        first = uint8(bytes(text)[0]);
        last = uint8(bytes(text)[32]);
        length = bytes(text).length;
    }

    function readOob(uint256 n) external returns (uint8) {
        _fill(n);
        return uint8(data[n]);
    }

    function writeOob(uint256 n) external {
        _fill(n);
        data[n] = 0x01;
    }

    function deleteOob(uint256 n) external {
        _fill(n);
        delete data[n];
    }
}
