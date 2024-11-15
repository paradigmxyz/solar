contract C {
    function C() public {} //~ ERROR: functions are not allowed to have the same name as the contract

    function c() public {}
}
