import "./emit_qualified_inherited.sol" as self;

interface B {
    event E(uint256 indexed a, bytes b);
    event F(uint256 a);
    error Er();
}

interface I is B {
    event E(uint64 a, bytes b);
}

// A contract name qualifier exposes only the contract's own events and errors.
library L {
    function f(uint64 x, bytes memory b) internal {
        emit I.E(x, b);
        emit self.I.E(x, b);
        emit B.E(x, b);
        emit I.F(1); //~ ERROR: unresolved symbol `F`
        emit self.I.F(1); //~ ERROR: unresolved symbol `F`
        revert I.Er(); //~ ERROR: unresolved symbol `Er`
    }
}

contract C is I {
    function f(uint64 x, bytes memory b) public {
        emit I.E(x, b);
        emit E(uint256(x), b);
        emit F(1);
        emit E(x, b); //~ ERROR: no unique declarations found
        emit C.E(x, b); //~ ERROR: unresolved symbol `E`
        revert C.Er(); //~ ERROR: unresolved symbol `Er`
    }
}
