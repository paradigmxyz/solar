//@ compile-flags: -Ztypeck
abstract contract AbstractContract {
    constructor() { }
    function utterance() public returns (bytes32) { return "miaow"; }
}

contract Test {
    function create() public {
       AbstractContract ac = new AbstractContract(); //~ ERROR: cannot instantiate
    }
}
