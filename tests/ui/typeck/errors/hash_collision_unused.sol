// Errors that a contract neither declares, inherits, nor uses are not part of its interface.
library L {
    error gsf();
}
contract test {
    error tgeo();
    function f() public pure {
        revert tgeo();
    }
}
