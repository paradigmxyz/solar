//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: probe 0x604d600c600039604d6000f37f11111111111111111111111111111111111111111111111111111111111111116000527f222222222222222222222222222222222222222222222222222222222222222260205260406000f3 => true, 32, 64, 0x1111111111111111111111111111111111111111111111111111111111111111
//@ run-call: probe 0x6029600c60003960296000f37f333333333333333333333333333333333333333333333333333333333333333360005260206000fd => false, 32, 32, 0x3333333333333333333333333333333333333333333333333333333333333333
//@ run-call: probeStatic 0x604d600c600039604d6000f37f11111111111111111111111111111111111111111111111111111111111111116000527f222222222222222222222222222222222222222222222222222222222222222260205260406000f3 => true, 32, 64, 0x1111111111111111111111111111111111111111111111111111111111111111
//@ run-call: probeDelegate 0x604d600c600039604d6000f37f11111111111111111111111111111111111111111111111111111111111111116000527f222222222222222222222222222222222222222222222222222222222222222260205260406000f3 => true, 32, 64, 0x1111111111111111111111111111111111111111111111111111111111111111
//@ run-call: probeWide 0x604d600c600039604d6000f37f11111111111111111111111111111111111111111111111111111111111111116000527f222222222222222222222222222222222222222222222222222222222222222260205260406000f3 => true, 64, 64, 0x2222222222222222222222222222222222222222222222222222222222222222, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: probeEmpty => true, 0, 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

// `Calls.callInto` copies at most the output buffer's length of what the
// callee returns, and reports both how much arrived and how much there was.
// A reverting callee gives `false` with its revert data in the buffer.
import {Bytes} from "solar:core/v1/Bytes.sol";
import {Calls} from "solar:core/v1/Calls.sol";
import {Create} from "solar:core/v1/Create.sol";

contract Test {
    function probe(bytes memory initcode)
        public
        returns (bool ok, uint256 copied, uint256 total, bytes32 first)
    {
        address target = Create.deploy(initcode, 0);
        bytes memory out = new bytes(32);
        (ok, copied, total) = Calls.callInto(target, 0, gasleft(), "", out);
        first = Bytes.readBytes32(out, 0);
    }

    function probeStatic(bytes memory initcode)
        public
        returns (bool ok, uint256 copied, uint256 total, bytes32 first)
    {
        address target = Create.deploy(initcode, 0);
        bytes memory out = new bytes(32);
        (ok, copied, total) = Calls.staticCallInto(target, gasleft(), "", out);
        first = Bytes.readBytes32(out, 0);
    }

    function probeDelegate(bytes memory initcode)
        public
        returns (bool ok, uint256 copied, uint256 total, bytes32 first)
    {
        address target = Create.deploy(initcode, 0);
        bytes memory out = new bytes(32);
        (ok, copied, total) = Calls.delegateCallInto(target, gasleft(), "", out);
        first = Bytes.readBytes32(out, 0);
    }

    function probeWide(bytes memory initcode)
        public
        returns (bool ok, uint256 copied, uint256 total, bytes32 second, bytes32 third)
    {
        address target = Create.deploy(initcode, 0);
        bytes memory out = new bytes(96);
        Bytes.fill(out, 0, 96, 0xff);
        (ok, copied, total) = Calls.callInto(target, 0, gasleft(), "", out);
        second = Bytes.readBytes32(out, 32);
        third = Bytes.readBytes32(out, 64);
    }

    function probeEmpty() public returns (bool ok, uint256 copied, uint256 total, bytes32 first) {
        bytes memory out = new bytes(32);
        Bytes.fill(out, 0, 32, 0xff);
        (ok, copied, total) = Calls.callInto(address(0xdead), 0, gasleft(), "", out);
        first = Bytes.readBytes32(out, 0);
    }
}
