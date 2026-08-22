//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: trySuccess() => 7
//@[none, gas, size] run-call: tryFailure() => 100
//@[none, gas, size] run-call: tryPair() => 7, "ok"

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

    function pair(Context memory context) external pure returns (uint256, string memory) {
        return (context.value, "ok");
    }

    function invokePair(
        function(Context memory) external returns (uint256, string memory) fn,
        Context memory context
    )
        internal
        returns (uint256, string memory)
    {
        try fn(context) returns (uint256 value, string memory text) {
            return (value, text);
        } catch {
            return (0, "failed");
        }
    }

    function trySuccess() external returns (uint256) {
        invoke(this.succeed, Context({value: 7}));
        return observed;
    }

    function tryFailure() external returns (uint256) {
        invoke(this.fail, Context({value: 7}));
        return observed;
    }

    function tryPair() external returns (uint256, string memory) {
        return invokePair(this.pair, Context({value: 7}));
    }
}
