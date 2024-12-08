contract U1 {
    function c() {} //~ERROR: No visibility specified. Did you intend to add public?
}

interface U2 {
    function c() {} //~ERROR: No visibility specified. Did you intend to add external?
}

contract U3 { //~ERROR: No visibility specified for fallback. Did you intend to add external?
    fallback() {}
}

contract U4 {//~ERROR: No visibility specified for receive. Did you intend to add external?
    receive() {}
}
