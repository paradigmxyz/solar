//@ compile-flags: -Ztypeck

library LibraryTypes {
    uint256 constant BASE = 1;

    type Amount is uint256;

    struct Item {
        uint256 x;
    }

    enum Choice {
        A,
        B
    }

    error Problem(uint256 value);

    event Signal(uint256 value);

    modifier only() {
        _;
    }

    function check() public pure {}
}

contract ContractTypes {
    uint256 constant BASE = 1;

    type Amount is uint256;

    struct Item {
        uint256 x;
    }

    enum Choice {
        A,
        B
    }

    error Problem(uint256 value);

    event Signal(uint256 value);

    modifier only() {
        _;
    }

    function check() public pure {}
}

contract C {
    function libraryTypes() public pure {
        LibraryTypes.BASE;
        LibraryTypes.Amount.wrap(1);
        LibraryTypes.Item(1);
        LibraryTypes.Choice.A;
        LibraryTypes.Problem.selector;
        LibraryTypes.Signal.selector;
        LibraryTypes.only; //~ ERROR: member `only` not found
        LibraryTypes.check.selector;
    }

    function contractTypes() public pure {
        ContractTypes.BASE; //~ ERROR: member `BASE` not found
        ContractTypes.Amount.wrap(1);
        ContractTypes.Item(1);
        ContractTypes.Choice.A;
        ContractTypes.Problem.selector;
        ContractTypes.Signal.selector;
        ContractTypes.only; //~ ERROR: member `only` not found
        ContractTypes.check.selector;
    }
}
