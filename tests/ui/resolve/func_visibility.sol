contract U1 {
    function c() {} //~ERROR: no visibility specified

    function d() public {}

    function e() external {}
}

interface U2 {
    function c() {} //~ERROR: no visibility specified
}

contract U3 {
    fallback() {} //~ERROR: no visibility specified
}

contract U4 {
    receive() {} //~ERROR: no visibility specified
}

contract U5 {
    fallback() external {}
}

contract U6 {
    receive() external {}
}
