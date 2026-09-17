//@ codegen-matrix: standard
//@ run-call: unconditional => 0
//@ run-call: conditional true => 0
//@ run-call: conditionalContinues => true

contract ConstructorStopRuntime {
    function unconditional() external returns (uint256) {
        ConstructorStopAlways deployed = new ConstructorStopAlways();
        return address(deployed).code.length;
    }

    function conditional(bool halt) external returns (uint256) {
        ConstructorStopConditional deployed = new ConstructorStopConditional(halt);
        return address(deployed).code.length;
    }

    function conditionalContinues() external returns (bool) {
        ConstructorStopConditional deployed = new ConstructorStopConditional(false);
        return address(deployed).code.length != 0;
    }
}

contract ConstructorStopAlways {
    uint256 public value;

    constructor() {
        value = 1;
        assembly {
            stop()
        }
    }
}

contract ConstructorStopConditional {
    uint256 public value;

    constructor(bool halt) {
        value = 1;
        if (halt) {
            assembly {
                stop()
            }
        }
    }
}
