//@ check-pass
contract C {
    function f() pure public { selfdestruct; }
}
