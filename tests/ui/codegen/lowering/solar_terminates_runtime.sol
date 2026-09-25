//@ codegen-matrix: standard
//@ run-call: check 3 => 4
//@ run-call-fail: check 11 => 0xa2f43130000000000000000000000000000000000000000000000000000000000000000b
//@ run-call: early true => "early"
//@ run-call: early false => "late"
//@ run-call-fail: pick 0 => 0xa2f431300000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: pick 1 => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000036f6e650000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: pick 7 => 0xa2f431300000000000000000000000000000000000000000000000000000000000000007
//@ run-call: guarded 2 => 3

// `@custom:solar-terminates` helpers behave as the same helpers without the
// tag: a revert with the helper's data, or a successful return of the whole
// call, and the caller's code after the call never runs.
import {Return} from "solar:core/v1/Return.sol";

contract Test {
    error Bad(uint256 code);

    uint256 locked;

    /// @custom:solar-terminates
    function fail(uint256 code) internal pure {
        revert Bad(code);
    }

    /// @custom:solar-terminates
    function failWith(uint256 code) internal pure returns (uint256) {
        if (code == 0) {
            revert Bad(0);
        } else if (code == 1) {
            revert("one");
        } else {
            fail(code);
        }
    }

    /// @custom:solar-terminates
    function finish(string memory s) internal pure {
        Return.abiEncoded(s);
    }

    function check(uint256 x) external pure returns (uint256) {
        if (x > 10) fail(x);
        return x + 1;
    }

    function early(bool done) external pure returns (string memory) {
        if (done) finish("early");
        return "late";
    }

    function pick(uint256 code) external pure returns (uint256) {
        return failWith(code) + 1;
    }

    modifier lock() {
        locked = 1;
        _;
        locked = 0;
    }

    // A reverting helper under a pending cleanup rolls the lock back.
    function guarded(uint256 x) external lock returns (uint256) {
        if (x > 5) fail(x);
        return x + locked;
    }
}
