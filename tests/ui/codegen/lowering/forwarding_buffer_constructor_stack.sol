//@ revisions: gas size
//@ compile-flags: --emit=bin
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@[size] normalize-stdout-test: "(?s).+" -> ""
//~[gas]? ERROR: codegen cannot preserve values across a low-memory forwarding buffer

struct S {
    uint24 a;
    uint256 b;
}

contract F {
    function g(uint24, uint256, int144) external returns (bool) {}
}

contract C {
    uint256[] aV;
    int144[] aW;
    bool[] aK;
    uint104[] aD;
    uint104[] aG;
    bool sT;
    uint24 immutable sI;
    uint256 sN;

    constructor() {
        uint256 bV = aV.length;
        uint256 bW = aW.length;
        uint256 bK = aK.length;
        uint256 bD = aD.length;
        uint256 bG = aG.length;
        new F().g(sI, 0, 0);
        assembly { returndatacopy(0, 0, returndatasize()) }
        uint104 t4 = aG[bG] ^ aG[bG + 1];
        bool t5 = !(aD[bD] < t4) || aK[bK];
        t5 = sT && !(new F().g(S(sI, sN).a, aV[bV], aW[bW]) || !(aD[bD] < (aG[bG] ^ aG[bG + 1])) || aK[bK]);
        sT = t5;
    }
}
