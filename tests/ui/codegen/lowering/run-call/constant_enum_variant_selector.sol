//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: variant => 1

// An enum variant can be named like the `selector` member, and constant evaluation folds it.
enum Named {
    a,
    selector
}

contract ConstantEnumVariantSelector {
    Named constant SELECTOR = Named.selector;

    // CHECK-LABEL: fn @variant()
    // CHECK: ret i8 1
    function variant() external pure returns (Named) {
        return SELECTOR;
    }
}
