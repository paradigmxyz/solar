//@ run-call: trySuccess() => 7
//@ run-call: tryFailure() => 100

contract ExternalFunctionPointerTry {
    struct Context {
        uint256 value;
    }

    uint256 private observed;

    function invoke(
        function(Context memory) external fn,
        Context memory context
    ) internal {
        try fn(context) {} catch (bytes memory reason) {
            observed = reason.length;
        }
    }

    function succeed(Context memory context) external {
        observed = context.value;
    }

    function fail(Context memory) external pure {
        revert("no");
    }

    function trySuccess() external returns (uint256) {
        invoke(this.succeed, Context({value: 7}));
        return observed;
    }

    function tryFailure() external returns (uint256) {
        invoke(this.fail, Context({value: 7}));
        return observed;
    }
}
