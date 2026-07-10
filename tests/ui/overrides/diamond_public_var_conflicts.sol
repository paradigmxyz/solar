// ported-from: test/libsolidity/syntaxTests/inheritance/override/public_vars_multiple_diamond1.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/diamond_interface_empty_intermediate_public_state_variable_and_function.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/diamond_interface_intermediate_public_state_variable_and_function.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/diamond_interface_intermediate_public_state_variable_and_function_implemented.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/diamond_top_implemented_intermediate_implemented_public_state_variable.sol

contract Top1 {
    function f1() external virtual view returns (uint) { return 5; }
}
contract Var1 is Top1 {
    uint public override f1;
    //~^ ERROR: cannot override non-virtual function
    //~| ERROR: cannot override public state variable
}
contract Function1 is Top1 {
    function f1() external virtual override view returns (uint) { return 5; }
}
contract Bottom1 is Var1, Function1 {
    uint public override f1;
    //~^ ERROR: identifier `f1` already declared
    //~| ERROR: Public state variable needs to specify overridden contracts
}

interface Empty2 {}
contract Var2 is Empty2 {
    uint public f2;
}
abstract contract Function2 is Empty2 {
    function f2() external virtual returns (uint); //~ ERROR: identifier `f2` already declared
}
abstract contract Bottom2 is Var2, Function2 {}
//~^ ERROR: derived contract must override function `f2`

interface Top3 {
    function f3() external returns (uint);
}
contract Var3 is Top3 {
    uint public f3;
}
abstract contract Function3 is Top3 {
    function f3() external virtual returns (uint); //~ ERROR: identifier `f3` already declared
}
abstract contract Bottom3 is Var3, Function3 {}
//~^ ERROR: derived contract must override function `f3`

interface Top4 {
    function f4() external returns (uint);
}
contract Var4 is Top4 {
    uint public f4;
}
abstract contract Function4 is Top4 {
    function f4() external virtual returns (uint) { return 2; } //~ ERROR: identifier `f4` already declared
}
abstract contract Bottom4 is Var4, Function4 {}
//~^ ERROR: derived contract must override function `f4`

contract Top5 {
    function f5() external view virtual returns (uint) { return 1; }
}
contract Var5 is Top5 {
    uint public override f5;
}
contract Function5 is Top5 {
    function f5() external pure virtual override returns (uint) { return 2; } //~ ERROR: identifier `f5` already declared
}
contract Bottom5 is Var5, Function5 {}
//~^ ERROR: derived contract must override function `f5`
