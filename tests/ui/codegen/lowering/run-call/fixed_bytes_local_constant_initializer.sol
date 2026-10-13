//@ codegen-matrix: standard
//@ run-call: fromBytes1 => 0x07
//@ run-call: fromBytes2 => 0x0102
//@ run-call: fromBytes4 => 0x01020304
//@ run-call: fromBytes16 => 0x0102030405060708090a0b0c0d0e0f10
//@ run-call: fromBytes32 => 0x0000000000000000000000000000000000000000000000000000000000000007
//@ run-call: fromHexConstant => 0x0102
//@ run-call: widened => 0x07000000
//@ run-call: cast => 0x0700
//@ run-call: fromNumberLiteral => 0x0102
//@ run-call: fromHexLiteral => 0x01020304

contract FixedBytesLocalConstantInitializer {
    bytes1 constant B1 = 0x07;
    bytes2 constant B2 = 0x0102;
    bytes4 constant B4 = 0x01020304;
    bytes16 constant B16 = 0x0102030405060708090a0b0c0d0e0f10;
    bytes32 constant B32 = 0x0000000000000000000000000000000000000000000000000000000000000007;
    bytes2 constant H2 = hex"0102";

    function fromBytes1() external pure returns (bytes1) {
        bytes1 value = B1;
        return value;
    }

    function fromBytes2() external pure returns (bytes2) {
        bytes2 value = B2;
        return value;
    }

    function fromBytes4() external pure returns (bytes4) {
        bytes4 value = B4;
        return value;
    }

    function fromBytes16() external pure returns (bytes16) {
        bytes16 value = B16;
        return value;
    }

    function fromBytes32() external pure returns (bytes32) {
        bytes32 value = B32;
        return value;
    }

    function fromHexConstant() external pure returns (bytes2) {
        bytes2 value = H2;
        return value;
    }

    function widened() external pure returns (bytes4) {
        bytes4 value = B1;
        return value;
    }

    function cast() external pure returns (bytes2) {
        bytes2 value = bytes2(B1);
        return value;
    }

    function fromNumberLiteral() external pure returns (bytes2) {
        bytes2 value = 0x0102;
        return value;
    }

    function fromHexLiteral() external pure returns (bytes4) {
        bytes4 value = hex"01020304";
        return value;
    }
}
