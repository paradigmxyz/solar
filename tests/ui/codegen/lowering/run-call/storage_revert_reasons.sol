//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call-fail: mappingShort => Error("x")
//@ run-call-fail: mappingLong => Error("abcdefghijklmnopqrstuvwxyz0123456")
//@ run-call-fail: mappingReference => Error("x")
//@ run-call-fail: nestedMapping => Error("x")
//@ run-call-fail: structRequire => Error("x")
//@ run-call-fail: calldataReason "x" => Error("x")

contract StorageRevertReasons {
    struct Holder {
        string reason;
    }

    mapping(uint256 => string) private reasons;
    mapping(uint256 => mapping(uint256 => string)) private nested;
    Holder private holder;

    function mappingShort() external {
        reasons[0] = "x";
        revert(reasons[0]);
    }

    function mappingLong() external {
        reasons[1] = "abcdefghijklmnopqrstuvwxyz0123456";
        revert(reasons[1]);
    }

    function mappingReference() external {
        reasons[2] = "x";
        string storage reason = reasons[2];
        revert(reason);
    }

    function nestedMapping() external {
        nested[0][1] = "x";
        revert(nested[0][1]);
    }

    function structRequire() external {
        holder.reason = "x";
        require(false, holder.reason);
    }

    function calldataReason(string calldata reason) external pure {
        revert(reason);
    }
}
