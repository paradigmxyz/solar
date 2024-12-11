abstract contract A {
    modifier x() {
        _;
    }

    function updateState() external virtual x; //~ERROR: functions without implementation cannot have modifiers
}

interface B {
    modifier x() {
        _;
    }

    function j() external x; //~ERROR: functions in interfaces cannot have modifiers
}
