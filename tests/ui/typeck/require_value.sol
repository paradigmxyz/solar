contract C {
    function g() pure public {
        function(bool) pure x = require; //~ ERROR: no matching declarations found
        x;
    }

    function h() pure public {
        require(true);
        require(true, "message");
    }

    function i() pure public returns (bytes memory) {
        return abi.encode(require); //~ ERROR: no matching declarations found
    }
}
