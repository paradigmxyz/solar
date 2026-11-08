// ported-from: test/libsolidity/natspecJSON/invalid/docstring_inheritdoc.sol
// ported-from: test/libsolidity/natspecJSON/invalid/docstring_inheritdoc3.sol
// ported-from: test/libsolidity/natspecJSON/invalid/docstring_inheritdoc_emptys.sol
// ported-from: test/libsolidity/natspecJSON/invalid/docstring_inheritdoc_wrong_type.sol

contract Inexistent {
    /// @inheritdoc X
    //~^ WARN: tag `@inheritdoc` references inexistent contract "X"
    function f() internal {
    }
}

contract D {
    struct S { uint a; }
}

contract BaseStruct is D {
    /// @inheritdoc D.S
    //~^ WARN: tag `@inheritdoc` reference `D.S` is not a contract
    function f() internal {
    }
}

contract Empty {
    /// @inheritdoc
    //~^ WARN: tag `@inheritdoc` does not contain a contract name
    function f() internal {
    }
    /// @inheritdoc .
    //~^ WARN: tag `@inheritdoc` reference `.` is malformed
    function g() internal {
    }
    /// @inheritdoc Empty..f
    //~^ WARN: tag `@inheritdoc` reference `Empty..f` is malformed
    function h() internal {
    }
    /// @inheritdoc Empty.
    //~^ WARN: tag `@inheritdoc` reference `Empty.` is malformed
    function i() internal {
    }
}

contract WrongType {
    struct S { uint a; }
    /// @inheritdoc S
    //~^ WARN: tag `@inheritdoc` reference `S` is not a contract
    function f() internal {
    }
}
