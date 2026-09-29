//@ revisions: intrinsic portable size
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[size] compile-flags: -Osize
//@ run-call: word 7 => 7
//@ run-call: signed -5 => -5
//@ run-call: account 0xffffffffffffffffffffffff1234567890123456789012345678901234567890 => 0x1234567890123456789012345678901234567890
//@ run-call: flag true => false
//@ run-call: flag false => true
//@ run-call: dirtyAccount 0xffffffffffffffffffffffff1234567890123456789012345678901234567890 => 0x1234567890123456789012345678901234567890
//@ run-call: dirtyFlag 2 => true
//@ run-call: dirtyFlag 0 => false
//@ run-call: hash 0x0102030405060708091011121314151617181920212223242526272829303132 => 0x0102030405060708091011121314151617181920212223242526272829303132
//@ run-call: blob 0x010203 => 0x010203
//@ run-call: blob 0x => 0x
//@ run-call: text "abc" => "abc"
//@ run-call: pointer 3 => 9
//@ run-call: 0xdeadbeef01 => 0xdeadbeef01
//@ run-call: 0x => 0x000000000000000000000000000000000000000000000000000000000000002a

// Each `Return.abiEncoded` overload ends the call with its argument encoded
// as the single result of a function returning that type: an address is
// cleaned to its width and a boolean to one bit, even when assembly left it
// dirty. A fallback function's output
// is raw bytes, so `Return.raw` ends a call to it with exactly its argument,
// and any encoding may end it too. A call through an internal function
// pointer ends the call it is made in.
import {Return} from "solar:core/v1/Return.sol";

contract Test {
    function word(uint256 value) public pure returns (uint256) {
        Return.abiEncoded(value);
    }

    function signed(int256 value) public pure returns (int256) {
        Return.abiEncoded(value);
    }

    function account(uint256 value) public pure returns (address) {
        Return.abiEncoded(address(uint160(value)));
    }

    function flag(bool value) public pure returns (bool) {
        Return.abiEncoded(!value);
    }

    function dirtyAccount(uint256 value) public pure returns (address) {
        address account_;
        assembly {
            account_ := value
        }
        Return.abiEncoded(account_);
    }

    function dirtyFlag(uint256 value) public pure returns (bool) {
        bool flag_;
        assembly {
            flag_ := value
        }
        Return.abiEncoded(flag_);
    }

    function hash(bytes32 value) public pure returns (bytes32) {
        Return.abiEncoded(value);
    }

    function blob(bytes calldata value) public pure returns (bytes memory) {
        Return.abiEncoded(value);
    }

    function text(string memory value) public pure returns (string memory) {
        Return.abiEncoded(value);
    }

    function pointer(uint256 n) public pure returns (uint256) {
        function(uint256) internal pure square = _square;
        square(n);
        return 0;
    }

    function _square(uint256 n) internal pure {
        Return.abiEncoded(n * n);
    }

    fallback() external {
        if (msg.data.length == 0) Return.abiEncoded(uint256(42));
        Return.raw(msg.data);
    }
}
