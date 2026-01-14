contract D {}

contract C {
    C c;
    D d;

    function foo() public {
        // Current instance of the current contract vs super
        super != this;
        this != super;

        // Different instance of the current contract vs super
        super != c;
        c != super;

        // Instance of an unrelated contract vs super
        super != d;
        d != super;
    }
}
