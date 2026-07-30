//@ run-call: trySuccess() => 7
//@ run-call: tryFailure() => 100
//@ run-call: callReturns() => 11
//@ run-call: passFunction() => 11
//@ run-call-fail: callEmptyReturn()
//@ run-call-fail: callVoidEoa()
//@ run-call: forgedViewStatic() => 1
//@ run-call: pointerSelector true => 0xa7a0d537
//@ run-call: pointerSelector false => 0x85295877
//@ run-call: pointerAddress() => true
//@ run-call: directAddress() => true
//@ run-call: selectorReceiverSideEffect() => 42
//@ run-call-fail: tryVoidEoa()
//@ run-call-fail: tryReturnEoa()
//@ run-call: tryForgedViewStatic() => 1
//@ run-call: callEvaluationOrder() => 1234
//@ run-call: pointerEvaluationOrder() => 1234
//@ run-call: tryEvaluationOrder() => 1234
//@ run-call: lowLevelEvaluationOrder() => 1234
//@ run-call: tryMemberFunctionPointer() => 11
//@ run-call: memberFunctionPointerSelector() => true

interface IVoid {
    function ping() external;
}

interface IView {
    function mutate() external view returns (uint256);
}

interface IReturns {
    function value() external view returns (uint256);
}

contract ExternalFunctionPointerTry {
    struct Context {
        uint256 value;
    }

    struct PointerHolder {
        function(uint256) external returns (uint256, bytes memory) fn;
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

    function values(uint256 value) external pure returns (uint256, bytes memory) {
        return (value, hex"010203");
    }

    function emptyReturn() external pure returns (uint256) {
        assembly {
            return(0, 0)
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

    function callReturns() external returns (uint256) {
        function(uint256) external returns (uint256, bytes memory) fn = this.values;
        (uint256 value, bytes memory data) = fn(7);
        return value + data.length + uint8(data[0]);
    }

    function accept(
        function(uint256) external returns (uint256, bytes memory) fn
    ) external returns (uint256) {
        (uint256 value, bytes memory data) = fn(7);
        return value + data.length + uint8(data[0]);
    }

    function passFunction() external returns (uint256) {
        return this.accept(this.values);
    }

    function callEmptyReturn() external returns (uint256) {
        function() external returns (uint256) fn = this.emptyReturn;
        return fn();
    }

    function callVoidEoa() external {
        function() external fn = IVoid(address(1)).ping;
        fn();
    }

    function mutate() external returns (uint256) {
        observed = 77;
        return observed;
    }

    function forgedViewStatic() external view returns (uint256) {
        function() external view returns (uint256) fn = IView(address(this)).mutate;
        try fn() returns (uint256) {
            return 0;
        } catch {
            return observed + 1;
        }
    }

    function something() external pure {}

    function other() external pure {}

    function pointerSelector(bool choose) external view returns (bytes4) {
        function() external pure fn = choose ? this.something : this.other;
        return fn.selector;
    }

    function pointerAddress() external view returns (bool) {
        function() external pure fn = this.something;
        return fn.address == address(this);
    }

    function directAddress() external pure returns (bool) {
        return ExternalFunctionPointerTry(address(0x1234)).something.address == address(0x1234);
    }

    function selectorReceiver() public returns (ExternalFunctionPointerTry) {
        observed = 42;
        return this;
    }

    function selectorReceiverSideEffect() external returns (uint256) {
        (selectorReceiver().something).selector;
        return observed;
    }

    function tryVoidEoa() external returns (uint256) {
        try IVoid(address(1)).ping() {
            return 0;
        } catch {
            return 1;
        }
    }

    function tryReturnEoa() external view returns (uint256) {
        try IReturns(address(0x1234)).value() {
            return 0;
        } catch {
            return 1;
        }
    }

    function tryForgedViewStatic() external view returns (uint256) {
        try IView(address(this)).mutate() returns (uint256) {
            return 0;
        } catch {
            return observed + 1;
        }
    }

    function orderReceiver() public returns (ExternalFunctionPointerTry) {
        observed = observed * 10 + 1;
        return this;
    }

    function orderValue() public returns (uint256) {
        observed = observed * 10 + 2;
        return 0;
    }

    function orderGas() public returns (uint256) {
        observed = observed * 10 + 3;
        return gasleft();
    }

    function orderArg() public returns (uint256) {
        observed = observed * 10 + 4;
        return 7;
    }

    function acceptOrder(uint256) external payable {}

    function callEvaluationOrder() external returns (uint256) {
        observed = 0;
        orderReceiver().acceptOrder{value: orderValue(), gas: orderGas()}(orderArg());
        return observed;
    }

    function pointerEvaluationOrder() external returns (uint256) {
        observed = 0;
        function(uint256) external payable fn = orderReceiver().acceptOrder;
        fn{value: orderValue(), gas: orderGas()}(orderArg());
        return observed;
    }

    function tryEvaluationOrder() external returns (uint256) {
        observed = 0;
        try orderReceiver().acceptOrder{value: orderValue(), gas: orderGas()}(orderArg()) {} catch {}
        return observed;
    }

    function lowLevelEvaluationOrder() external returns (uint256) {
        observed = 0;
        (bool success,) = address(orderReceiver()).call{value: orderValue(), gas: orderGas()}(
            abi.encodeCall(this.acceptOrder, (orderArg()))
        );
        require(success);
        return observed;
    }

    function tryMemberFunctionPointer() external returns (uint256) {
        PointerHolder memory holder = PointerHolder(this.values);
        try holder.fn(7) returns (uint256 value, bytes memory data) {
            return value + data.length + uint8(data[0]);
        } catch {
            return 0;
        }
    }

    function memberFunctionPointerSelector() external view returns (bool) {
        PointerHolder memory holder = PointerHolder(this.values);
        return holder.fn.selector == this.values.selector;
    }
}
