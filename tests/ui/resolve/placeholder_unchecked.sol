contract A {
    modifier xrp() {
        unchecked {
            _; //~ERROR: placeholder statements cannot be used inside unchecked blocks
        }
    }

    constructor() {}
}
