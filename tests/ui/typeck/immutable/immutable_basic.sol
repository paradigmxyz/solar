// Valid immutable declarations
contract C {
    uint immutable x = 0;
    uint immutable y;
    
    constructor() {
        y = 42;
    }
}
