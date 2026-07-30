//@ compile-flags: --emit=abi --pretty-json

event ReferencedEvent(uint256 value);
event ModifierEvent();
event UnusedEvent();

error ReferencedError();
error InitializerError();
error UnusedError();

event BaseHookEvent();
event DerivedHookEvent();
event FunctionPointerEvent();

function initialize() view returns (uint256) {
    if (block.timestamp == 0) {
        revert InitializerError();
    }
    return 1;
}

function referenced(uint256 value) {
    emit ReferencedEvent(value);
    if (value == 0) {
        revert ReferencedError();
    }
}

contract External {
    event ExternalEvent();

    function emitExternal() external {
        emit ExternalEvent();
    }
}

contract Base {
    event BaseEvent();
    error BaseError();
}

contract C is Base {
    uint256 value = initialize();

    constructor() {
        callReferenced();
    }

    modifier records() {
        emit ModifierEvent();
        _;
    }

    function run(uint256 newValue) external records {
        callReferenced();
        value = newValue;
    }

    function callExternal(External externalContract) external {
        externalContract.emitExternal();
    }

    function callReferenced() private {
        referenced(value);
    }

    function unused() private {
        emit UnusedEvent();
        revert UnusedError();
    }
}

contract VirtualBase {
    function execute() external {
        hook();
    }

    function hook() internal virtual {
        emit BaseHookEvent();
    }
}

contract VirtualDerived is VirtualBase {
    function hook() internal override {
        emit DerivedHookEvent();
    }
}

contract FunctionPointer {
    function run() external {
        function() internal target = emitEvent;
        target();
    }

    function emitEvent() internal {
        emit FunctionPointerEvent();
    }
}
