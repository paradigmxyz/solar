//@compile-flags: -Ztypeck

contract C {
    function exponentiationRightAssociative(uint[2**3**2] memory a) internal pure {
        uint[512] memory b = a;
    }

    function subtractionLeftAssociative(uint[8 - 3 - 2] memory a) internal pure {
        uint[3] memory b = a;
    }
}
