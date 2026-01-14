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
    receive() payable {} //~ERROR: no visibility specified
}

contract U5 {
    fallback() external {}
}

contract U6 {
    receive() external payable {}
}

contract U7 {
    constructor() {}
}

function uvw() internal {} //~ERROR: free functions cannot have visibility

function xyz(); //~ERROR: free functions must be implemented
