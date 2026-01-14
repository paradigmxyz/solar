contract Base {
    constructor(uint) {}
}
contract Derived is Base(2) { }
