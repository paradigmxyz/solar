import "./type_paths.sol" as self;

contract C {
    struct S {
        uint x;
    }

    function f(
        S memory a,
        C.S memory b,
        self.C.S memory c,
        self.C.Unknown memory d
        //~^ unresolved symbol
    ) public {
        S memory e = S(0);
        C.S memory f = C.S(1);
        self.C.S memory g = self.C.S(2);
        
        self.C.Unknown memory h = self.C.Unknown(3);
        //~^ unresolved symbol
    }
}
