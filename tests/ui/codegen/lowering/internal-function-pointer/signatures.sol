//@ run-call: callVoid => true
//@ run-call: callState => true
//@ run-call: callPair 7 => 7, 8
//@ run-call: callTwoArgs => 6
//@ run-call-fail: callZero => 0x4e487b710000000000000000000000000000000000000000000000000000000000000051

contract FunctionPointerSignatures {
    bool flag;
    function() internal stateFn = setFlag;

    function callVoid() public returns (bool) {
        function() internal fn = setFlag;
        fn();
        return flag;
    }

    function setFlag() public {
        flag = true;
    }

    function callState() public returns (bool) {
        stateFn();
        return flag;
    }

    function callPair(uint256 value) public returns (uint256, uint256) {
        function(uint256) internal returns (uint256, uint256) fn = pair;
        return fn(value);
    }

    function pair(uint256 value) internal pure returns (uint256, uint256) {
        return (value, value + 1);
    }

    function callZero() public {
        function() internal fn;
        fn();
    }

    function sum(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }

    function callTwoArgs() public returns (uint256) {
        function(uint256, uint256) internal returns (uint256) sumFn = sum;
        return sumFn(5, 1);
    }
}
