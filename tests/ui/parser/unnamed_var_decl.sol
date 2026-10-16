contract C {
    function f() public {
        function () ; //~ ERROR: expected identifier, found `;`
    }
}
