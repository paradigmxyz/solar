contract C {
    function g() pure public {
        function(bool) pure x = require; //~ ERROR: no matching declarations found
        x;
    }

    function h() pure public {
        require(true);
        require(true, "message");
    }
}
