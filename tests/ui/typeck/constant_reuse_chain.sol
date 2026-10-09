//@ check-pass
// Each constant uses the previous one three times, so evaluating every use again would take
// 3**39 steps. Constant evaluation computes each constant once.
contract ConstantReuseChain {
    uint256 constant C0 = 1;
    uint256 constant C1 = C0 + C0 - C0;
    uint256 constant C2 = C1 + C1 - C1;
    uint256 constant C3 = C2 + C2 - C2;
    uint256 constant C4 = C3 + C3 - C3;
    uint256 constant C5 = C4 + C4 - C4;
    uint256 constant C6 = C5 + C5 - C5;
    uint256 constant C7 = C6 + C6 - C6;
    uint256 constant C8 = C7 + C7 - C7;
    uint256 constant C9 = C8 + C8 - C8;
    uint256 constant C10 = C9 + C9 - C9;
    uint256 constant C11 = C10 + C10 - C10;
    uint256 constant C12 = C11 + C11 - C11;
    uint256 constant C13 = C12 + C12 - C12;
    uint256 constant C14 = C13 + C13 - C13;
    uint256 constant C15 = C14 + C14 - C14;
    uint256 constant C16 = C15 + C15 - C15;
    uint256 constant C17 = C16 + C16 - C16;
    uint256 constant C18 = C17 + C17 - C17;
    uint256 constant C19 = C18 + C18 - C18;
    uint256 constant C20 = C19 + C19 - C19;
    uint256 constant C21 = C20 + C20 - C20;
    uint256 constant C22 = C21 + C21 - C21;
    uint256 constant C23 = C22 + C22 - C22;
    uint256 constant C24 = C23 + C23 - C23;
    uint256 constant C25 = C24 + C24 - C24;
    uint256 constant C26 = C25 + C25 - C25;
    uint256 constant C27 = C26 + C26 - C26;
    uint256 constant C28 = C27 + C27 - C27;
    uint256 constant C29 = C28 + C28 - C28;
    uint256 constant C30 = C29 + C29 - C29;
    uint256 constant C31 = C30 + C30 - C30;
    uint256 constant C32 = C31 + C31 - C31;
    uint256 constant C33 = C32 + C32 - C32;
    uint256 constant C34 = C33 + C33 - C33;
    uint256 constant C35 = C34 + C34 - C34;
    uint256 constant C36 = C35 + C35 - C35;
    uint256 constant C37 = C36 + C36 - C36;
    uint256 constant C38 = C37 + C37 - C37;
    uint256 constant C39 = C38 + C38 - C38;

    uint256[C39] lengths;
}
