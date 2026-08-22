//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: sameShortLiteralSalt() => true
//@[gas] run-call: sameShortLiteralSalt() => true
//@[size] run-call: sameShortLiteralSalt() => true
//@[none] run-call: sameEmptyLiteralSalt() => true
//@[gas] run-call: sameEmptyLiteralSalt() => true
//@[size] run-call: sameEmptyLiteralSalt() => true
//@[none] run-call: sameFullLiteralSalt() => true
//@[gas] run-call: sameFullLiteralSalt() => true
//@[size] run-call: sameFullLiteralSalt() => true
//@[none] run-call: differentLiteralSalts() => true
//@[gas] run-call: differentLiteralSalts() => true
//@[size] run-call: differentLiteralSalts() => true

contract CreationOptionsChild {}

contract CreationOptions {
    function sameShortLiteralSalt() external returns (bool) {
        new CreationOptionsChild{salt: "xyz"}();
        try new CreationOptionsChild{salt: "xyz"}() {
            return false;
        } catch {
            return true;
        }
    }

    function sameEmptyLiteralSalt() external returns (bool) {
        new CreationOptionsChild{salt: ""}();
        try new CreationOptionsChild{salt: ""}() {
            return false;
        } catch {
            return true;
        }
    }

    function sameFullLiteralSalt() external returns (bool) {
        new CreationOptionsChild{salt: "12345678901234567890123456789012"}();
        try new CreationOptionsChild{salt: "12345678901234567890123456789012"}() {
            return false;
        } catch {
            return true;
        }
    }

    function differentLiteralSalts() external returns (bool) {
        CreationOptionsChild first = new CreationOptionsChild{salt: "abc"}();
        CreationOptionsChild second = new CreationOptionsChild{salt: "def"}();
        return first != second;
    }
}
