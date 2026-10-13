//@ revisions: intrinsic portable mir
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] compile-flags: -Ogas -Zdump=mir
//@[mir] filecheck:
//@ run-call: store 0x0102030405 => 0x000102030405, 0x0102030405
//@ run-call: store 0x => 0x00, 0x
//@ run-call: storeAt 0x0102030405, 0x0000000000000000000000000000000000000000000000000000000000000007 => true, 0x000102030405
//@ run-call: attempt 0x0102030405 => true, 0x000102030405
//@ run-call: echo 0x0102030405 => 0x123456780102030405, 0x0102030405
//@ run-call: echoWord 0xaabb => 0x0000000000000000000000000000000000000000000000000000000000000007aabb, 0xaabb
//@ run-call: echoBare 0xaabb => 0xaabb, 0xaabb
//@ run-call: echoSigned 0xaabb => 0xffaabb, 0xaabb
//@ run-call: emptyStaysEmpty => 0x00, 0

// An `abi.encodePacked` input to a creation or a bounded call that ends with
// bytes in memory, after at most a word of literals and scalars, is laid out
// over the length word of those bytes instead of copied: the prefix goes where
// the length was, and the length is written back once the creation or call has
// read the input. The bytes read back unchanged afterwards, including the
// zero word all empty bytes share. The initcode is Solady's SSTORE2 data
// contract: it returns a STOP byte and the data after it as the code.
// CHECK-LABEL: fn @store(
// CHECK-NOT: mcopy
// CHECK: create
// CHECK-LABEL: fn @storeAt(
// CHECK-NOT: mcopy
// CHECK: keccak256
// CHECK-NOT: mcopy
// CHECK: create2
// CHECK-LABEL: fn @echo(
// CHECK-NOT: mcopy
// CHECK: = call
import {Calls} from "solar:core/Calls.sol";
import {Create} from "solar:core/Create.sol";

contract Test {
    // PUSH2 l, DUP1, PUSH1 0x0a, RETURNDATASIZE, CODECOPY, RETURNDATASIZE, RETURN, STOP
    function store(bytes memory data) public returns (bytes memory code, bytes memory kept) {
        address pointer = Create.deploy(
            abi.encodePacked(hex"61", uint16(data.length + 1), hex"80600a3d393df300", data), 0
        );
        return (pointer.code, data);
    }

    function storeAt(bytes memory data, bytes32 salt)
        public
        returns (bool predicted, bytes memory code)
    {
        bytes32 hash = keccak256(
            abi.encodePacked(hex"61", uint16(data.length + 1), hex"80600a3d393df300", data)
        );
        address pointer = Create.deploy2(
            abi.encodePacked(hex"61", uint16(data.length + 1), hex"80600a3d393df300", data),
            salt,
            0
        );
        return (pointer == Create.predict2(address(this), salt, hash), pointer.code);
    }

    function attempt(bytes memory data) public returns (bool ok, bytes memory code) {
        address pointer;
        (ok, pointer) = Create.tryDeploy(
            abi.encodePacked(hex"61", uint16(data.length + 1), hex"80600a3d393df300", data), 0
        );
        code = pointer.code;
    }

    // The identity precompile returns its input.
    function echo(bytes memory data) public returns (bytes memory output, bytes memory kept) {
        (, output,) = Calls.callBounded(
            address(4), 0, gasleft(), abi.encodePacked(bytes4(0x12345678), data), 100
        );
        kept = data;
    }

    function echoWord(bytes memory data) public returns (bytes memory output, bytes memory kept) {
        (, output,) = Calls.callBounded(
            address(4), 0, gasleft(), abi.encodePacked(bytes32(uint256(7)), data), 100
        );
        kept = data;
    }

    function echoBare(bytes memory data)
        public
        view
        returns (bytes memory output, bytes memory kept)
    {
        (, output,) = Calls.staticCallBounded(address(4), gasleft(), abi.encodePacked(data), 100);
        kept = data;
    }

    function echoSigned(bytes memory data)
        public
        view
        returns (bytes memory output, bytes memory kept)
    {
        int8 minusOne = -1;
        (, output,) = Calls.staticCallBounded(
            address(4), gasleft(), abi.encodePacked(minusOne, data), 100
        );
        kept = data;
    }

    function emptyStaysEmpty() public returns (bytes memory code, uint256 length) {
        bytes memory empty;
        address pointer = Create.deploy(
            abi.encodePacked(hex"61", uint16(1), hex"80600a3d393df300", empty), 0
        );
        assembly {
            length := mload(0x60)
        }
        return (pointer.code, length);
    }
}
