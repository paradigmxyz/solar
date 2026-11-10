//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: conditional => 0x03
//@ run-call: scalarRecursion => 0x06
//@ run-call: memoryRecursion => 0x03
//@ run-call: failingHelper false => 0x03
//@ run-call-fail: failingHelper true
//@ run-call: partition 0 => 0x03
//@ run-call: partition 1 => 0x03
//@ run-call: partition 2 => 0x03
//@ run-call: partition 9 => 0x03
//@ run-call: reloadedBound 0x010203 => 0x010203
//@ run-call: reloadedBound 0x => 0x
//@ run-call: selected true => 0x03
//@ run-call: selected false => 0x03
import {Buffers, ByteBuilder} from "solar:core/Buffers.sol";

contract Test {
    function recursiveBytes(uint256 n) internal pure returns (bytes memory) {
        if (n == 0) return hex"03";
        return recursiveBytes(n - 1);
    }
    function memoryRecursion() public pure returns (bytes memory) {
        ByteBuilder memory b = Buffers.create(1);
        Buffers.append(b, recursiveBytes(3));
        return Buffers.finish(b);
    }

    // Assembly in an unrelated entry must not disable another entry's typed-memory proof.
    function unrelatedAssembly() external pure returns (uint256 x) {
        assembly ("memory-safe") { x := 1 }
    }

    function factorial(uint256 n) internal pure returns (uint256) {
        return n == 0 ? 1 : n * factorial(n - 1);
    }
    function scalarRecursion() public pure returns (bytes memory) {
        ByteBuilder memory b = Buffers.create(1);
        Buffers.appendByte(b, bytes1(uint8(factorial(3))));
        return Buffers.finish(b);
    }

    function maybe(ByteBuilder memory b, bool done) internal pure {
        if (done) Buffers.finish(b);
    }

    function conditional() public pure returns (bytes memory) {
        ByteBuilder memory b = Buffers.create(4);
        maybe(b, false);
        Buffers.appendByte(b, 0x03);
        return Buffers.finish(b);
    }

    function fail(ByteBuilder memory b, bool abort) internal pure {
        if (abort) {
            Buffers.finish(b);
            revert();
        }
    }

    function failingHelper(bool abort) public pure returns (bytes memory) {
        ByteBuilder memory b = Buffers.create(4);
        fail(b, abort);
        Buffers.appendByte(b, 0x03);
        return Buffers.finish(b);
    }

    function partition(uint256 x) public pure returns (bytes memory out) {
        ByteBuilder memory b = Buffers.create(4);
        Buffers.appendByte(b, 0x03);
        if (x == 0) out = Buffers.finish(b);
        if (x == 1) out = Buffers.finish(b);
        if (x == 2) out = Buffers.finish(b);
        if (x > 2) out = Buffers.finish(b);
    }

    function reloadedBound(bytes memory parts) public pure returns (bytes memory out) {
        ByteBuilder memory b = Buffers.create(parts.length);
        for (uint256 i; i < parts.length; ++i) {
            Buffers.appendByte(b, parts[i]);
            if (i + 1 == parts.length) out = Buffers.finish(b);
        }
    }

    function selected(bool flag) public pure returns (bytes memory) {
        ByteBuilder memory a = Buffers.create(4);
        ByteBuilder memory b = Buffers.create(4);
        ByteBuilder memory selectedBuilder = flag ? a : b;
        Buffers.finish(selectedBuilder);
        if (flag) {
            Buffers.appendByte(b, 0x03);
            return Buffers.finish(b);
        }
        Buffers.appendByte(a, 0x03);
        return Buffers.finish(a);
    }
}
