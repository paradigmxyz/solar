//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: values 7 => 128, 128
//@ run-call: byteStrings 0x => 128, 128
//@ run-call: byteStrings 0x01 => 192, 192
//@ run-call: byteStrings 0x0102030405060708091011121314151617181920212223242526272829303132 => 192, 192
//@ run-call: byteStrings 0x010203040506070809101112131415161718192021222324252627282930313233 => 256, 256
//@ run-call: text "" => 192, 192
//@ run-call: text "a string longer than one word of thirty-two bytes" => 256, 256
//@ run-call: words [] => 128, 128
//@ run-call: words [1, 2, 3] => 320, 320
//@ run-call: points [(1, 0x1111111111111111111111111111111111111111), (2, 0x2222222222222222222222222222222222222222)] => 192, 192
//@ run-call: fixedWords [1, 2, 3] => 128, 128
//@ run-call: nested [0x01, 0x, 0x010203040506070809101112131415161718192021222324252627282930313233] => 352, 352
//@ run-call: order (5, 0x0102) => 160, 160
//@ run-call: prefixed 9, 0x0102 => 132, 132, 132
//@ run-call: counted => 68, 2
//@ run-call: viewed 0x00000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000301020300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000b => 224, 224
//@ run-call: plain 0x010203 => 3
//@ run-call: enums (1, 1), [0, 1] => 192, 192
//@ run-call-fail: dirtyEnumField => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: dirtyEnumElement => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: dirtyEnumValue => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021

// `Abi.encodedSize` with an `abi.encode*` call as its argument is the length
// that encoding has, computed from the arguments' lengths without encoding
// them for values, byte strings, arrays of values and static aggregates, and
// measured from an encoding staged past the free memory pointer otherwise.
// Each function returns it beside the length of the encoding itself, and the
// arguments are evaluated either way. Any other argument is measured as it is.
import {Abi} from "solar:core/Abi.sol";

contract Test {
    struct Point {
        uint64 x;
        address y;
    }

    struct Order {
        uint256 id;
        bytes payload;
    }

    enum Side {
        Buy,
        Sell
    }

    struct Quote {
        uint256 price;
        Side side;
    }

    uint256 private calls;

    function target(uint256, bytes memory) external pure {}

    function values(uint256 a) public pure returns (uint256, uint256) {
        return (
            Abi.encodedSize(abi.encode(a, address(0), true, bytes32(0))),
            abi.encode(a, address(0), true, bytes32(0)).length
        );
    }

    function byteStrings(bytes calldata b) external pure returns (uint256, uint256) {
        bytes memory m = b;
        return (Abi.encodedSize(abi.encode(b, m)), abi.encode(b, m).length);
    }

    function text(string memory s) public pure returns (uint256, uint256) {
        return (Abi.encodedSize(abi.encode(s, "literal", 1)), abi.encode(s, "literal", 1).length);
    }

    function words(uint256[] calldata xs) external pure returns (uint256, uint256) {
        uint256[] memory m = xs;
        return (Abi.encodedSize(abi.encode(xs, m)), abi.encode(xs, m).length);
    }

    function points(Point[] memory ps) public pure returns (uint256, uint256) {
        return (Abi.encodedSize(abi.encode(ps)), abi.encode(ps).length);
    }

    function fixedWords(uint256[3] memory xs) public pure returns (uint256, uint256) {
        return (Abi.encodedSize(abi.encode(xs, uint8(1))), abi.encode(xs, uint8(1)).length);
    }

    function nested(bytes[] memory items) public pure returns (uint256, uint256) {
        return (Abi.encodedSize(abi.encode(items)), abi.encode(items).length);
    }

    function order(Order memory o) public pure returns (uint256, uint256) {
        return (Abi.encodedSize(abi.encode(o)), abi.encode(o).length);
    }

    function prefixed(uint256 a, bytes memory b) public view returns (uint256, uint256, uint256) {
        return (
            Abi.encodedSize(abi.encodeWithSelector(0x12345678, a, b)),
            Abi.encodedSize(abi.encodeWithSignature("target(uint256,bytes)", a, b)),
            Abi.encodedSize(abi.encodeCall(this.target, (a, b)))
        );
    }

    function counted() public returns (uint256, uint256) {
        return (Abi.encodedSize(abi.encodeWithSelector(0x12345678, bump(), bump())), calls);
    }

    function bump() internal returns (uint256) {
        return ++calls;
    }

    function viewed(bytes memory data) public pure returns (uint256, uint256) {
        /// @custom:solar-view
        (bytes memory b, uint256[] memory xs) = abi.decode(data, (bytes, uint256[]));
        return (Abi.encodedSize(abi.encode(b, xs)), abi.encode(b, xs).length);
    }

    function plain(bytes memory b) public pure returns (uint256) {
        return Abi.encodedSize(b);
    }

    function enums(Quote memory q, Side[] memory sides) public pure returns (uint256, uint256) {
        return (Abi.encodedSize(abi.encode(q, sides)), abi.encode(q, sides).length);
    }

    // The encoding checks the range of every enum it encodes, so a size that comes from an
    // encoding that fails fails the same way, with Panic(0x21).
    function dirtyEnumField() public pure returns (uint256) {
        Quote memory q = Quote(1, Side.Buy);
        assembly {
            mstore(add(q, 0x20), 5)
        }
        return Abi.encodedSize(abi.encode(q));
    }

    function dirtyEnumElement() public pure returns (uint256) {
        Side[] memory sides = new Side[](2);
        assembly {
            mstore(add(sides, 0x40), 7)
        }
        return Abi.encodedSize(abi.encode(sides));
    }

    function dirtyEnumValue() public pure returns (uint256) {
        Side side;
        assembly {
            side := 9
        }
        return Abi.encodedSize(abi.encode(side));
    }
}
