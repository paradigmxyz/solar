contract PublicStateVarBase {
    uint256 public baseVar = 42;
}

contract PublicStateVarDerived is PublicStateVarBase {
    uint256 public selfVar = 24;

    function getStateVars() public view returns (uint256 a, uint256 b, uint256 c, uint256 d) {
        a = PublicStateVarDerived.selfVar;
        b = PublicStateVarBase.baseVar;
        c = this.selfVar();
        d = this.baseVar();

        this.selfVar;
        this.baseVar;

        PublicStateVarDerived.selfVar(); //~ ERROR: expected function, found `uint256`
        PublicStateVarBase.baseVar(); //~ ERROR: expected function, found `uint256`
        super.baseVar; //~ ERROR: member `baseVar` not found
        super.baseVar(); //~ ERROR: member `baseVar` not found
    }
}

contract PublicStateVarOtherScope {
    function getStateVars() public view {
        PublicStateVarDerived.selfVar; //~ ERROR: member `selfVar` not found
        PublicStateVarDerived.baseVar; //~ ERROR: member `baseVar` not found
        PublicStateVarBase.baseVar; //~ ERROR: member `baseVar` not found

        PublicStateVarDerived.selfVar(); //~ ERROR: member `selfVar` not found
        PublicStateVarDerived.baseVar(); //~ ERROR: member `baseVar` not found
        PublicStateVarBase.baseVar(); //~ ERROR: member `baseVar` not found
    }
}

contract QualifiedLvalueBase {
    uint256 public baseVar = 42;
    uint256 public constant constantVar = 1;
    uint256 public immutable immutableVar;

    constructor() {
        QualifiedLvalueBase.immutableVar = 2;
    }
}

contract QualifiedLvalueDerived is QualifiedLvalueBase {
    uint256 public selfVar = 24;
    uint256 public immutable derivedImmutable;

    constructor() {
        QualifiedLvalueDerived.derivedImmutable = 3;
    }

    function setStateVars() public {
        QualifiedLvalueDerived.selfVar = 1;
        QualifiedLvalueBase.baseVar = 2;

        QualifiedLvalueBase.constantVar = 3; //~ ERROR: cannot assign to a constant variable
        QualifiedLvalueBase.immutableVar = 4; //~ ERROR: cannot assign to immutable here
        QualifiedLvalueDerived.derivedImmutable = 5; //~ ERROR: cannot assign to immutable here
    }
}
