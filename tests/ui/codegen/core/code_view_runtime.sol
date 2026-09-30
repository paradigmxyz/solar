//@ revisions: intrinsic portable size
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[size] compile-flags: -Osize
//@ run-call: viewed 0x67010203040506070860005260086018f3, 0, 8 => 0x0102030405060708
//@ run-call: viewed 0x67010203040506070860005260086018f3, 2, 3 => 0x030405
//@ run-call: viewed 0x67010203040506070860005260086018f3, 8, 0 => 0x
//@ run-call: nested 0x67010203040506070860005260086018f3, 1, 6, 2, 3 => 0x040506
//@ run-call: fields 0x67010203040506070860005260086018f3, 3, 4 => true, 3, 4
//@ run-call: copied 0x67010203040506070860005260086018f3 => 0xaa0506aa
//@ run-call-fail: viewed 0x67010203040506070860005260086018f3, 6, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: viewed 0x67010203040506070860005260086018f3, 115792089237316195423570985008687907853269984665640564039457584007913129639935, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: nested 0x67010203040506070860005260086018f3, 1, 6, 4, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: overflowing 0x67010203040506070860005260086018f3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: arrived 79228162514264337593543950337 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// `Code.slice` makes a `CodeView` of a range that lies inside an account's
// code, and of a range inside another view; `read` and `copyInto` copy it,
// checking the range against the code again, so a view that arrives from
// outside, here one of an account without code, fails where it is read.
import {Code, CodeView} from "solar:core/Code.sol";
import {Create} from "solar:core/Create.sol";

contract Test {
    function viewed(bytes memory initcode, uint256 start, uint256 count)
        public
        returns (bytes memory)
    {
        CodeView section = Code.slice(Create.deploy(initcode, 0), start, count);
        return Code.read(section);
    }

    function nested(bytes memory initcode, uint256 start, uint256 count, uint256 inner, uint256 innerCount)
        public
        returns (bytes memory)
    {
        CodeView section = Code.slice(Create.deploy(initcode, 0), start, count);
        return Code.read(Code.slice(section, inner, innerCount));
    }

    function fields(bytes memory initcode, uint256 start, uint256 count)
        public
        returns (bool, uint256, uint256)
    {
        address target = Create.deploy(initcode, 0);
        CodeView section = Code.slice(target, start, count);
        return (Code.account(section) == target, Code.offset(section), Code.length(section));
    }

    function copied(bytes memory initcode) public returns (bytes memory out) {
        out = hex"aaaaaaaa";
        Code.copyInto(out, 1, Code.slice(Create.deploy(initcode, 0), 4, 2));
    }

    function overflowing(bytes memory initcode) public returns (bytes memory out) {
        out = new bytes(1);
        Code.copyInto(out, 0, Code.slice(Create.deploy(initcode, 0), 0, 2));
    }

    function arrived(CodeView section) public view returns (bytes memory) {
        return Code.read(section);
    }
}
