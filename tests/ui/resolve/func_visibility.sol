contract U1 {
    function c() {} //~ERROR: no visibility specified
}

interface U2 {
    function c() {} //~ERROR: no visibility specified
}

contract U3 { //~ERROR: no visibility specified for fallback
    fallback() {}
}

contract U4 {//~ERROR: no visibility specified for receive
    receive() {}
}

contract U5 { 
    fallback() external {}
}

contract U6 {
    receive() external {}
}
