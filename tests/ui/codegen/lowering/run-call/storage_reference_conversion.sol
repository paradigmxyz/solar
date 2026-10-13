//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: structField => 5
//@ run-call: element => 7
//@ run-call: length => 2
//@ run-call: bindAndPush => 2, 7
//@ run-call: fixedSize => 4
//@ run-call: mappingValue => 3
//@ run-call: structMember => 8
//@ run-call: deleteElement => 0
//@ run-call: ternary true => 1
//@ run-call: ternary false => 0
//@ run-call: memoryCopy => 1
//@ run-call: returnArray => [1, 2]
//@ run-call: localReference => 5
//@ run-call: stringLength => 5
//@ run-call: stringWrite => true
//@ run-call: stringPush => 6
//@ run-call: bytesToString => true
//@ run-call: bytesToStringConcat => true
//@ run-call: returnString => "abc"
//@ run-call: encodeString => true
//@ run-call: assignMemory => true
//@ run-call: concatBytes => true
//@ run-call-fail: revertMessage => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000026e6f000000000000000000000000000000000000000000000000000000000000
//@ run-call: fixedBytesValue => 0x11223344

interface I {
    struct S {
        uint256 x;
    }
}

contract StorageReferenceConversion {
    struct T {
        uint256[] ys;
    }

    I.S[] records;
    uint256[] nums;
    uint256[] other;
    uint256[3] fixedNums;
    mapping(uint256 => uint256[]) arrays;
    T t;
    string str;
    bytes bs;
    bytes4 b4;

    // The conversion result still refers to storage, so writes through it must reach the state
    // variables rather than a memory copy.
    function structField() external returns (uint256) {
        records.push();
        I.S[](records)[0].x = 5;
        return records[0].x;
    }

    function element() external returns (uint256) {
        nums.push();
        uint256[](nums)[0] = 7;
        return nums[0];
    }

    function length() external returns (uint256) {
        nums.push(1);
        nums.push(2);
        return uint256[](nums).length;
    }

    function bindAndPush() external returns (uint256, uint256) {
        nums.push(1);
        uint256[] storage p = uint256[](nums);
        p.push(7);
        return (nums.length, nums[1]);
    }

    function fixedSize() external returns (uint256) {
        fixedNums[1] = 4;
        return uint256[3](fixedNums)[1];
    }

    function mappingValue() external returns (uint256) {
        uint256[](arrays[1]).push(3);
        return arrays[1][0];
    }

    function structMember() external returns (uint256) {
        uint256[](t.ys).push(8);
        return t.ys[0];
    }

    function deleteElement() external returns (uint256) {
        nums.push(1);
        delete uint256[](nums)[0];
        return nums[0];
    }

    function ternary(bool c) external returns (uint256) {
        nums.push(1);
        return (c ? uint256[](nums) : other).length;
    }

    function memoryCopy() external returns (uint256) {
        nums.push(1);
        uint256[] memory copy = uint256[](nums);
        copy[0] = 9;
        return nums[0];
    }

    function returnArray() external returns (uint256[] memory) {
        nums.push(1);
        nums.push(2);
        return uint256[](nums);
    }

    function localReference() external returns (uint256) {
        uint256[] storage p = nums;
        p.push(5);
        return uint256[](p)[0];
    }

    function stringLength() external returns (uint256) {
        str = "hello";
        return bytes(str).length;
    }

    function stringWrite() external returns (bool) {
        str = "hello";
        bytes(str)[0] = "j";
        return keccak256(bytes(str)) == keccak256("jello");
    }

    function stringPush() external returns (uint256) {
        str = "hello";
        bytes storage p = bytes(str);
        p.push("x");
        return bytes(str).length;
    }

    function bytesToString() external returns (bool) {
        bs = "abc";
        string memory s = string(bs);
        return keccak256(bytes(s)) == keccak256("abc");
    }

    function bytesToStringConcat() external returns (bool) {
        bs = "abc";
        return keccak256(bytes(string.concat(string(bs), "d"))) == keccak256("abcd");
    }

    function returnString() external returns (string memory) {
        bs = "abc";
        return string(bs);
    }

    function encodeString() external returns (bool) {
        bs = "abc";
        return keccak256(abi.encode(string(bs))) == keccak256(abi.encode("abc"));
    }

    function assignMemory() external returns (bool) {
        bs = "abc";
        string memory s;
        s = string(bs);
        return keccak256(bytes(s)) == keccak256("abc");
    }

    function concatBytes() external returns (bool) {
        str = "ab";
        return keccak256(bytes.concat(bytes(str), "c")) == keccak256("abc");
    }

    function revertMessage() external {
        bs = "no";
        revert(string(bs));
    }

    function fixedBytesValue() external returns (bytes4) {
        b4 = 0x11223344;
        return bytes4(b4);
    }
}
