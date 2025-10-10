abstract contract NoArgs {
    uint256 private a;
    constructor() {
        a = 0;
    }
}
abstract contract WithArgs {
    uint256 private a;
    constructor(uint256 b) {
        a = b;
    }
}

abstract contract D is NoArgs {}

abstract contract E is NoArgs {
    constructor() {}
}

abstract contract F is NoArgs {
    constructor() NoArgs {} //~ERROR: modifier-style base constructor call without arguments
}
abstract contract FF is NoArgs {
    constructor() NoArgs() {} // OK
}

contract J is WithArgs(1337) { //~HELP: previous declaration
    constructor() WithArgs(1337) {} //~ERROR: base constructor arguments given twice
}

contract G is WithArgs {
    constructor(uint x) WithArgs(x) {}
}

// ok
contract K is WithArgs(1337) {
    constructor() {}
}

// ensure `this` can be used in base constructor inheritance
// ref: https://github.com/paradigmxyz/solar/issues/569
abstract contract WithAddrArgs {
    constructor(address) {}
}

contract L is WithAddrArgs(address(this)) {
    constructor() {}
}

contract M is WithAddrArgs(address(this)), WithArgs(42) {
    constructor() {}
}
