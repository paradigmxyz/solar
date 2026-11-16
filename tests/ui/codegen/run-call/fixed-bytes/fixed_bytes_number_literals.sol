//@ codegen-matrix: standard
//@ run-call: FixedBytesLiterals::viaExternal => 16909060
//@ run-call: FixedBytesLiterals::viaInternal => 0x11223344
//@ run-call: FixedBytesLiterals::viaLibrary => 0x21222324
//@ run-call: FixedBytesLiterals::retLit => 0xaabbccdd
//@ run-call: FixedBytesLiterals::declLit => 0x55667788
//@ run-call: FixedBytesLiterals::assignLit => 0x66778899
//@ run-call: FixedBytesLiterals::mapKeyCross => 5
//@ run-call: FixedBytesLiterals::compareControl => true
//@ run-call: FixedBytesLiterals::bytesObjectCast => 0x93dafdf1
//@ run-call: FixedBytesLiterals::returnedBytesObjectCast => 14
//@ run-call: FixedBytesLiterals::calldataSliceCast => 0x1122334455000000000000000000000000000000000000000000000000000000
//@ run-call: FixedBytesLiterals::stringAddressCast => 0x6d696c6164790000000000000000000000000000
//@ run-call: FixedBytesLiterals::emptyStringCompare => true
//@ run-call: FixedBytesLiterals::stateLiterals => 0x11223344, 0xaabb, 0x55667788

// A bare numeric literal used where `bytesN` is expected keeps its numeric
// sema type, so plain lowering yields the right-aligned integer word; every
// `bytesN` consumer wants the content bytes at the top. Cover each position
// a literal reaches a fixed-bytes target.

interface IProbe {
    function probe(bytes4 s) external returns (uint256);
}

contract Probe is IProbe {
    function probe(bytes4 s) external pure returns (uint256) {
        return uint32(s);
    }
}

library LibGive {
    function give(bytes4 s) internal pure returns (bytes4) {
        return s;
    }
}

contract FixedBytesLiterals {
    mapping(bytes4 => uint256) internal m;
    bytes4 internal stateLiteral = 0x11223344;
    bytes2 internal stateNeighbor = 0xaabb;
    bytes4 internal immutable immutableLiteral = 0x55667788;

    function viaExternal() public returns (uint256) {
        return IProbe(address(new Probe())).probe(0x01020304);
    }

    function giveInternal(bytes4 s) internal pure returns (bytes4) {
        return s;
    }

    function viaInternal() public pure returns (bytes4) {
        return giveInternal(0x11223344);
    }

    function viaLibrary() public pure returns (bytes4) {
        return LibGive.give(0x21222324);
    }

    function retLit() public pure returns (bytes4) {
        return 0xaabbccdd;
    }

    function declLit() public pure returns (bytes4) {
        bytes4 x = 0x55667788;
        return x;
    }

    function assignLit() public pure returns (bytes4 x) {
        x = 0x66778899;
    }

    // A literal key must hash to the same slot as the equivalent typed value.
    function mapKeyCross() public returns (uint256) {
        m[0x31323334] = 5;
        bytes4 key = 0x31323334;
        return m[key];
    }

    function compareControl() public pure returns (bool) {
        bytes4 x = bytes4(uint32(0x41424344));
        return x == 0x41424344;
    }

    function bytesObjectCast() public pure returns (bytes4) {
        bytes memory data = hex"93dafdf1aabb";
        return bytes4(data);
    }

    function returnedBytesObjectCast() public pure returns (uint256) {
        (, bytes memory data) = returnBytes();
        return uint256(bytes32(data)) >> 248;
    }

    function returnBytes() internal pure returns (bool, bytes memory) {
        return (true, hex"0e");
    }

    function takeSlice(bytes calldata data) external pure returns (bytes32) {
        return bytes32(data[:5]);
    }

    function calldataSliceCast() public view returns (bytes32) {
        return this.takeSlice(hex"112233445566778899");
    }

    function stringAddressCast() public pure returns (address) {
        return address(bytes20("milady"));
    }

    function emptyStringCompare() public pure returns (bool) {
        bytes32 value;
        return value == "";
    }

    function stateLiterals() public view returns (bytes4, bytes2, bytes4) {
        return (stateLiteral, stateNeighbor, immutableLiteral);
    }
}
