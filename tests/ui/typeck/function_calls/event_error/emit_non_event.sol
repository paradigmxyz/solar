// ported-from: test/libsolidity/syntaxTests/emit/emit_non_event.sol

contract EmitNonEvent {
    function() Test;

    function emitNonEvent() public {
        emit Test(); //~ ERROR: expression has to be an event invocation
    }
}
