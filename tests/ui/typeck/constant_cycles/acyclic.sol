//@ check-pass
// Shared dependencies are not cycles.

library L {
    uint constant public X = 1;
}

contract C {
    uint constant A = B + D;
    uint constant B = D;
    uint constant D = L.X;
    uint constant E = A * B + L.X;
    uint constant G = H;
    uint constant H = 1;

    function f() public pure returns (uint t) {
        t = E;
        assembly {
            t := G
        }
    }
}
