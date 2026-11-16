// Like solc, array lengths do not take conversions, `type(T).max`, conditionals, builtins, member
// accesses of constants or constants nested more than 32 deep.

import "./auxiliary/array_len_unsupported.sol" as M;

library L {
    uint8 constant C = 4;
}

uint constant C0 = 1;
uint constant C1 = C0 + 1;
uint constant C2 = C1 + 1;
uint constant C3 = C2 + 1;
uint constant C4 = C3 + 1;
uint constant C5 = C4 + 1;
uint constant C6 = C5 + 1;
uint constant C7 = C6 + 1;
uint constant C8 = C7 + 1;
uint constant C9 = C8 + 1;
uint constant C10 = C9 + 1;
uint constant C11 = C10 + 1;
uint constant C12 = C11 + 1;
uint constant C13 = C12 + 1;
uint constant C14 = C13 + 1;
uint constant C15 = C14 + 1;
uint constant C16 = C15 + 1;
uint constant C17 = C16 + 1;
uint constant C18 = C17 + 1;
uint constant C19 = C18 + 1;
uint constant C20 = C19 + 1;
uint constant C21 = C20 + 1;
uint constant C22 = C21 + 1;
uint constant C23 = C22 + 1;
uint constant C24 = C23 + 1;
uint constant C25 = C24 + 1;
uint constant C26 = C25 + 1;
uint constant C27 = C26 + 1;
uint constant C28 = C27 + 1;
uint constant C29 = C28 + 1;
uint constant C30 = C29 + 1;
uint constant C31 = C30 + 1;
uint constant C32 = C31 + 1;
uint constant C33 = C32 + 1;
uint constant C34 = C33 + 1;
uint constant C35 = C34 + 1;
uint constant C36 = C35 + 1;
uint constant C37 = C36 + 1;
uint constant C38 = C37 + 1;
uint constant C39 = C38 + 1;
uint constant C40 = C39 + 1;

contract C {
    function lengths() external pure {
        uint[uint8(3)] memory a; //~ ERROR: failed to evaluate constant: unsupported expression
        uint[type(uint8).max] memory b; //~ ERROR: failed to evaluate constant: unsupported expression
        uint[true ? 1 : 2] memory c; //~ ERROR: failed to evaluate constant: unsupported expression
        uint[uint(keccak256("x")) % 4 + 1] memory d; //~ ERROR: failed to evaluate constant: unsupported expression
        uint[L.C] memory e; //~ ERROR: failed to evaluate constant: unsupported expression
        uint[M.C] memory f; //~ ERROR: failed to evaluate constant: unsupported expression
        uint[C40] memory g; //~ ERROR: failed to evaluate constant: constants are nested too deeply
        uint[uint32(bytes4(0x00000004))] memory h; //~ ERROR: failed to evaluate constant: unsupported expression
        uint[C0 < 2 ? 1 : 2] memory i; //~ ERROR: failed to evaluate constant: unsupported expression
        a;
        b;
        c;
        d;
        e;
        f;
        g;
        h;
        i;
    }
}
