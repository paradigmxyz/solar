//@ codegen-matrix: standard
//@ run-call: decimal 0 => "0"
//@ run-call: decimal 1 => "1"
//@ run-call: decimal 9 => "9"
//@ run-call: decimal 10 => "10"
//@ run-call: decimal 99 => "99"
//@ run-call: decimal 100 => "100"
//@ run-call: decimal 12345 => "12345"
//@ run-call: decimal 1000000000000000000 => "1000000000000000000"
//@ run-call: decimal 9999999999999999999999999999999999999999999999999999999999999999 => "9999999999999999999999999999999999999999999999999999999999999999"
//@ run-call: decimal 10000000000000000000000000000000000000000000000000000000000000000 => "10000000000000000000000000000000000000000000000000000000000000000"
//@ run-call: decimal 115792089237316195423570985008687907853269984665640564039457584007913129639935 => "115792089237316195423570985008687907853269984665640564039457584007913129639935"
//@ run-call: signed 0 => "0"
//@ run-call: signed 1 => "1"
//@ run-call: signed -1 => "-1"
//@ run-call: signed -10 => "-10"
//@ run-call: signed 57896044618658097711785492504343953926634992332820282019728792003956564819967 => "57896044618658097711785492504343953926634992332820282019728792003956564819967"
//@ run-call: signed -57896044618658097711785492504343953926634992332820282019728792003956564819968 => "-57896044618658097711785492504343953926634992332820282019728792003956564819968"
//@ run-call: escape 0x => 0x
//@ run-call: escape 0x706c61696e2074657874 => 0x706c61696e2074657874
//@ run-call: escape 0x7361792022686922 => 0x736179205c2268695c22
//@ run-call: escape 0x6261636b5c736c617368 => 0x6261636b5c5c736c617368
//@ run-call: escape 0x7461620968657265 => 0x7461625c7468657265
//@ run-call: escape 0x6c696e650a627265616b => 0x6c696e655c6e627265616b
//@ run-call: escape 0x011f => 0x5c75303030315c7530303166
//@ run-call: escape 0x080c0d => 0x5c625c665c72
//@ run-call: escape 0x6161616161616161616161616161616161616161616161616161616161616161616161616161616122 => 0x616161616161616161616161616161616161616161616161616161616161616161616161616161615c22
//@ run-call: same "abc", "abc" => true
//@ run-call: same "abc", "abd" => false
//@ run-call: same "abc", "abcd" => false
//@ run-call: same "", "" => true

// `Strings` is source code over `Bytes` and `Buffers`. `escapeJSON` builds its
// output in a `ByteBuilder`, which grows when escapes make the text longer;
// its input and output go through `bytes` here so that control characters
// survive the directive.
import {Strings} from "solar:core/v1/Strings.sol";

contract Test {
    function decimal(uint256 value) public pure returns (string memory) {
        return Strings.toString(value);
    }

    function signed(int256 value) public pure returns (string memory) {
        return Strings.toString(value);
    }

    function escape(bytes memory s) public pure returns (bytes memory) {
        return bytes(Strings.escapeJSON(string(s)));
    }

    function same(string memory a, string memory b) public pure returns (bool) {
        return Strings.equals(a, b);
    }
}
