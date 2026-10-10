contract B {
    uint x;

    modifier m() {
        _;
        x = 2;
        x = 3;
    }
}
