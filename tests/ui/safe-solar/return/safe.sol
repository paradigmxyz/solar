//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: echo "" => ""
//@ run-call: echo "a" => "a"
//@ run-call: echo "0123456789abcdef0123456789abcdef" => "0123456789abcdef0123456789abcdef"
//@ run-call: echo "0123456789abcdef0123456789abcdef!" => "0123456789abcdef0123456789abcdef!"
//@ run-call: fromHelper 3 => "xyz"
//@ run-call: stopsHere 5 => "early"
//@ run-call: literal => "hello"

import {Return} from "solar:core/v1/Return.sol";

contract Safe {
    uint256 public counter;

    function echo(string memory s) public pure returns (string memory) {
        Return.abiEncoded(s);
    }

    // The call ends inside the helper, not at the helper's return.
    function fromHelper(uint256 n) public pure returns (string memory) {
        _finish(n);
        return "unreachable";
    }

    function _finish(uint256 n) private pure {
        bytes memory b = new bytes(n);
        for (uint256 i; i < n; ++i) {
            b[i] = bytes1(uint8(0x78 + i));
        }
        Return.abiEncoded(string(b));
    }

    // Nothing after the return runs: the counter stays unchanged.
    function stopsHere(uint256 n) public returns (string memory) {
        Return.abiEncoded("early");
        counter = n;
        return "late";
    }

    // A literal that was never allocated returns its bytes.
    function literal() public pure returns (string memory) {
        Return.abiEncoded("hello");
    }
}
