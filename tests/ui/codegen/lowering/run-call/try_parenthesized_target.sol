//@ codegen-matrix: standard
//@ run-call: Caller::success => 5
//@ run-call: Caller::failure => 2
// Parenthesizing a `try` target changes nothing about the call it wraps, so
// both the checker and lowering peel the parentheses. solc requires the target
// to be a call syntactically and rejects the form; see TYPECK-005 in
// docs/SOLC_DIVERGENCE.md.
contract Target {
    function ok() external pure returns (uint256) {
        return 5;
    }

    function bad() external pure returns (uint256) {
        revert("no");
    }
}

contract Caller {
    Target private target;

    constructor() {
        target = new Target();
    }

    function success() external view returns (uint256) {
        try (target.ok()) returns (uint256 v) {
            return v;
        } catch {
            return 1;
        }
    }

    function failure() external view returns (uint256) {
        try ((target.bad())) returns (uint256 v) {
            return v;
        } catch {
            return 2;
        }
    }
}
