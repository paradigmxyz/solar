//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: IndexedEventFunctionBytes::emitEvent()

contract IndexedEventFunctionBytes {
    struct Payload {
        function() external callback;
        bytes data;
    }

    event Emitted(Payload indexed payload);

    function target() external {}

    function emitEvent() external {
        Payload memory payload;
        payload.callback = this.target;
        payload.data = "x";
        emit Emitted(payload);
    }
}
