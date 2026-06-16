// ported-from: test/libsolidity/syntaxTests/inheritance/override/ambiguous_base_functions_overridden_in_intermediate_base.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/ambiguous_base_functions_overridden_in_intermediate_base_unimplemented.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/correct_choice_for_base_function.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/interface_and_base_override_err.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/interface_and_base_override_fine.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/no_common_base_and_unique_implementation.sol

// ==== Valid: intermediate override resolves ambiguity ====
contract ResolvedA {
    function f() external virtual {}
}
contract ResolvedB {
    function f() external virtual {}
}
contract ResolvedC is ResolvedA, ResolvedB {
    function f() external override(ResolvedA, ResolvedB) {}
}
contract ResolvedX is ResolvedC {}

// ==== Invalid: unimplemented intermediate override does not resolve implemented bases ====
contract UnimplementedA {
    function f() external virtual {}
}
contract UnimplementedB {
    function f() external virtual {}
}
abstract contract UnimplementedC is UnimplementedA, UnimplementedB {
    function f() external override(UnimplementedA, UnimplementedB);
    //~^ ERROR: functions without implementation must be marked virtual
    //~| ERROR: cannot override implemented function with unimplemented function
    //~| ERROR: cannot override implemented function with unimplemented function
}
abstract contract UnimplementedX is UnimplementedC {}

// ==== Valid: pick the interface and concrete base, not the intermediate contract ====
interface IBaseChoice {
    function foo() external view;
}
contract BaseChoice is IBaseChoice {
    function foo() public virtual view {}
}
interface IExtChoice is IBaseChoice {}
contract ExtChoice is IExtChoice, BaseChoice {}
contract TChoice {
    function foo() public virtual view {}
}
contract ChoiceImpl is ExtChoice, TChoice {
    function foo() public view override(IBaseChoice, BaseChoice, TChoice) {}
}

// ==== Invalid: interface and base override list still required ====
contract MissingChoiceImpl is ExtChoice {
    function foo() public view {}
    //~^ ERROR: overriding function is missing `override` specifier
    //~| ERROR: Function needs to specify overridden contracts
}

// ==== Invalid: no common base, each branch has a unique implementation ====
abstract contract NoCommonA {
    function f() external {}
    function g() external virtual;
}
abstract contract NoCommonB {
    function g() external {}
    function f() external virtual;
}
contract NoCommonC is NoCommonA, NoCommonB {}
//~^ ERROR: derived contract must override function `f`
//~| ERROR: derived contract must override function `g`
