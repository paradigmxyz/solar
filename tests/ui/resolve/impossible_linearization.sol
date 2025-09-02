abstract contract ParentA {
    constructor(uint x) {}
}
abstract contract ParentB {
    constructor(bool y) {}
}
abstract contract Sub is ParentA(0), ParentB {
    constructor() ParentB(true) {}
}

contract ListsA is Sub, ParentA {}
//~^ ERROR: linearization of inheritance graph impossible
contract ListsB is Sub, ParentB {}
//~^ ERROR: linearization of inheritance graph impossible
contract ListsBoth is Sub, ParentA, ParentB {}
//~^ ERROR: linearization of inheritance graph impossible
