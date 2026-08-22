//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: IndexedEventFunctionBytes::emitEvent()

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
