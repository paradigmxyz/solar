//@ check-pass
contract ArrayExpressions {
    function() internal internal $0;
    /* */function/* */()/* */internal/* */internal/* */$1/* */;

    function test() external pure {
        uint[]memory a;
        /* */uint/* */[]/* */memory/* */b;
        /** */uint/** */[]/** */memory/** */c;
    }
}
