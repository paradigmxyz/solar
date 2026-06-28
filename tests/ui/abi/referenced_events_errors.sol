//@ compile-flags: --emit=abi --pretty-json -Ztypeck
//@ filecheck: --implicit-check-not=UnusedEvent --implicit-check-not=UnusedError

// CHECK-LABEL: "ROOT/tests/ui/abi/referenced_events_errors.sol:C": {
// CHECK: "type": "constructor"
// CHECK: "name": "GlobalError"
// CHECK: "name": "HelperError"
// CHECK: "name": "OperatorError"
// CHECK: "name": "BaseDeclared"
// CHECK: "name": "GlobalEvent"
// CHECK: "anonymous": false
// CHECK: "name": "HelperEvent"
// CHECK: "anonymous": true
// CHECK: "name": "add"
// CHECK: "name": "fail"
// CHECK: "name": "trigger"

event GlobalEvent(address indexed sender, uint256 value);
event HelperEvent(uint256 indexed value) anonymous;
event UnusedEvent();

error GlobalError(uint256 value);
error HelperError(address account);
error OperatorError(uint256 value);
error UnusedError();

type MyValue is uint256;

function plus(MyValue a, MyValue b) pure returns (MyValue) {
    uint256 value = MyValue.unwrap(a);
    if (value == 0) {
        revert OperatorError(MyValue.unwrap(b));
    }
    return MyValue.wrap(value + MyValue.unwrap(b));
}

using {plus as +} for MyValue global;

contract Base {
    event BaseDeclared(uint256 value);
}

contract C is Base {
    modifier emitsFromModifier(uint256 value) {
        emit HelperEvent(value);
        _;
    }

    constructor(uint256 initialValue) {
        emit GlobalEvent(msg.sender, initialValue);
    }

    function trigger(uint256 value) public emitsFromModifier(value) {
        helper(value);
    }

    function fail(uint256 value) public pure {
        revert GlobalError(value);
    }

    function add(MyValue a, MyValue b) public pure returns (MyValue) {
        return a + b;
    }

    function helper(uint256 value) internal {
        emit GlobalEvent(msg.sender, value);
        if (value == 0) {
            revert HelperError(msg.sender);
        }
    }

    function unused() private {
        emit UnusedEvent();
        revert UnusedError();
    }
}
