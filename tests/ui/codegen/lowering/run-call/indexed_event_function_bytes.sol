//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
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
