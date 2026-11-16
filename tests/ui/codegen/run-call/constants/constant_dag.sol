//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: ConstantDag::f => 354224848179261915075
//@ run-call: AddmodDag::f => 102334155

// Each constant reads the next two, so the initializers form a DAG with an
// exponential number of paths. Lowering `C0` must visit each constant once,
// whether or not constant evaluation folds it.
contract ConstantDag {
    uint constant C99 = 1;
    uint constant C98 = 1;
    uint constant C97 = C98 + C99;
    uint constant C96 = C97 + C98;
    uint constant C95 = C96 + C97;
    uint constant C94 = C95 + C96;
    uint constant C93 = C94 + C95;
    uint constant C92 = C93 + C94;
    uint constant C91 = C92 + C93;
    uint constant C90 = C91 + C92;
    uint constant C89 = C90 + C91;
    uint constant C88 = C89 + C90;
    uint constant C87 = C88 + C89;
    uint constant C86 = C87 + C88;
    uint constant C85 = C86 + C87;
    uint constant C84 = C85 + C86;
    uint constant C83 = C84 + C85;
    uint constant C82 = C83 + C84;
    uint constant C81 = C82 + C83;
    uint constant C80 = C81 + C82;
    uint constant C79 = C80 + C81;
    uint constant C78 = C79 + C80;
    uint constant C77 = C78 + C79;
    uint constant C76 = C77 + C78;
    uint constant C75 = C76 + C77;
    uint constant C74 = C75 + C76;
    uint constant C73 = C74 + C75;
    uint constant C72 = C73 + C74;
    uint constant C71 = C72 + C73;
    uint constant C70 = C71 + C72;
    uint constant C69 = C70 + C71;
    uint constant C68 = C69 + C70;
    uint constant C67 = C68 + C69;
    uint constant C66 = C67 + C68;
    uint constant C65 = C66 + C67;
    uint constant C64 = C65 + C66;
    uint constant C63 = C64 + C65;
    uint constant C62 = C63 + C64;
    uint constant C61 = C62 + C63;
    uint constant C60 = C61 + C62;
    uint constant C59 = C60 + C61;
    uint constant C58 = C59 + C60;
    uint constant C57 = C58 + C59;
    uint constant C56 = C57 + C58;
    uint constant C55 = C56 + C57;
    uint constant C54 = C55 + C56;
    uint constant C53 = C54 + C55;
    uint constant C52 = C53 + C54;
    uint constant C51 = C52 + C53;
    uint constant C50 = C51 + C52;
    uint constant C49 = C50 + C51;
    uint constant C48 = C49 + C50;
    uint constant C47 = C48 + C49;
    uint constant C46 = C47 + C48;
    uint constant C45 = C46 + C47;
    uint constant C44 = C45 + C46;
    uint constant C43 = C44 + C45;
    uint constant C42 = C43 + C44;
    uint constant C41 = C42 + C43;
    uint constant C40 = C41 + C42;
    uint constant C39 = C40 + C41;
    uint constant C38 = C39 + C40;
    uint constant C37 = C38 + C39;
    uint constant C36 = C37 + C38;
    uint constant C35 = C36 + C37;
    uint constant C34 = C35 + C36;
    uint constant C33 = C34 + C35;
    uint constant C32 = C33 + C34;
    uint constant C31 = C32 + C33;
    uint constant C30 = C31 + C32;
    uint constant C29 = C30 + C31;
    uint constant C28 = C29 + C30;
    uint constant C27 = C28 + C29;
    uint constant C26 = C27 + C28;
    uint constant C25 = C26 + C27;
    uint constant C24 = C25 + C26;
    uint constant C23 = C24 + C25;
    uint constant C22 = C23 + C24;
    uint constant C21 = C22 + C23;
    uint constant C20 = C21 + C22;
    uint constant C19 = C20 + C21;
    uint constant C18 = C19 + C20;
    uint constant C17 = C18 + C19;
    uint constant C16 = C17 + C18;
    uint constant C15 = C16 + C17;
    uint constant C14 = C15 + C16;
    uint constant C13 = C14 + C15;
    uint constant C12 = C13 + C14;
    uint constant C11 = C12 + C13;
    uint constant C10 = C11 + C12;
    uint constant C9 = C10 + C11;
    uint constant C8 = C9 + C10;
    uint constant C7 = C8 + C9;
    uint constant C6 = C7 + C8;
    uint constant C5 = C6 + C7;
    uint constant C4 = C5 + C6;
    uint constant C3 = C4 + C5;
    uint constant C2 = C3 + C4;
    uint constant C1 = C2 + C3;
    uint constant C0 = C1 + C2;

    // CHECK-LABEL: fn @f()
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 0x1333db76a7c594bfc3
    function f() external pure returns (uint) {
        return C0;
    }
}

contract AddmodDag {
    uint constant M = 1000000007;
    uint constant D39 = 1;
    uint constant D38 = 1;
    uint constant D37 = addmod(D38, D39, M);
    uint constant D36 = addmod(D37, D38, M);
    uint constant D35 = addmod(D36, D37, M);
    uint constant D34 = addmod(D35, D36, M);
    uint constant D33 = addmod(D34, D35, M);
    uint constant D32 = addmod(D33, D34, M);
    uint constant D31 = addmod(D32, D33, M);
    uint constant D30 = addmod(D31, D32, M);
    uint constant D29 = addmod(D30, D31, M);
    uint constant D28 = addmod(D29, D30, M);
    uint constant D27 = addmod(D28, D29, M);
    uint constant D26 = addmod(D27, D28, M);
    uint constant D25 = addmod(D26, D27, M);
    uint constant D24 = addmod(D25, D26, M);
    uint constant D23 = addmod(D24, D25, M);
    uint constant D22 = addmod(D23, D24, M);
    uint constant D21 = addmod(D22, D23, M);
    uint constant D20 = addmod(D21, D22, M);
    uint constant D19 = addmod(D20, D21, M);
    uint constant D18 = addmod(D19, D20, M);
    uint constant D17 = addmod(D18, D19, M);
    uint constant D16 = addmod(D17, D18, M);
    uint constant D15 = addmod(D16, D17, M);
    uint constant D14 = addmod(D15, D16, M);
    uint constant D13 = addmod(D14, D15, M);
    uint constant D12 = addmod(D13, D14, M);
    uint constant D11 = addmod(D12, D13, M);
    uint constant D10 = addmod(D11, D12, M);
    uint constant D9 = addmod(D10, D11, M);
    uint constant D8 = addmod(D9, D10, M);
    uint constant D7 = addmod(D8, D9, M);
    uint constant D6 = addmod(D7, D8, M);
    uint constant D5 = addmod(D6, D7, M);
    uint constant D4 = addmod(D5, D6, M);
    uint constant D3 = addmod(D4, D5, M);
    uint constant D2 = addmod(D3, D4, M);
    uint constant D1 = addmod(D2, D3, M);
    uint constant D0 = addmod(D1, D2, M);

    // CHECK-LABEL: fn @f()
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 0x6197ecb
    function f() external pure returns (uint) {
        return D0;
    }
}
