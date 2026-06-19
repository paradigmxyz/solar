//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/revertStatement/revert_event.sol

contract RevertEvent {
    event EmptyEvent();

    function revertEvent() public pure {
        revert EmptyEvent(); //~ ERROR: event invocations have to be prefixed by `emit`
        //~^ ERROR: expression has to be an error
    }
}
