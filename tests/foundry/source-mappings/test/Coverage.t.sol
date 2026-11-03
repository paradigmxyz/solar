// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import {Harness, Base} from "../src/Base.sol";
import {Decode} from "../src/Decode.sol";
import {StaticBytes} from "../src/StaticBytes.sol";

contract CoverageTest {
    function testInit() external {
        Harness h = new Harness();
        h.init(address(1));
        assert(h.a() == address(1));
        (bool ok, bytes memory reason) = address(h).call(abi.encodeCall(h.init, (address(0))));
        assert(!ok && bytes4(reason) == Base.ZeroAddress.selector);
    }

    function testDecodeA() external {
        Decode d = new Decode();
        (address who, uint256 got) = d.execute(7, abi.encode(Decode.Op.A, abi.encode(address(42), uint256(5))));
        assert(who == address(42) && got == 12 && d.stored() == 12);
    }

    function testDecodeB() external {
        Decode d = new Decode();
        (address who, uint256 got) = d.execute(7, abi.encode(Decode.Op.B, bytes("")));
        assert(who == address(0) && got == 0);
    }

    function testInitRejectsZero() external {
        Harness h = new Harness();
        (bool ok, bytes memory reason) = address(h).call(abi.encodeCall(h.init, (address(0))));
        assert(!ok && bytes4(reason) == Base.ZeroAddress.selector);
        assert(h.a() == address(0));
    }

    function testStaticEncoding() external {
        StaticBytes s = new StaticBytes();
        assert(keccak256(s.encoded()) == keccak256(abi.encode(uint256(7), uint256(11))));
    }

    function testEmptyEncoding() external {
        StaticBytes s = new StaticBytes();
        assert(s.empty().length == 0);
    }

    function testLiteral() external {
        StaticBytes s = new StaticBytes();
        assert(keccak256(s.literal()) == keccak256(hex"0102"));
    }
}
