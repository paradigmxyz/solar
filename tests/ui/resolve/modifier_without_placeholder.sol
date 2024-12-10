contract U {
    modifier P() {} //~ERROR: modifier must have a `_;` placeholder statement
}

contract C {
    modifier P() {
        if (true) {
            _;
        }
    }
}
