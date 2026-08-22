//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: sameShortLiteralSalt() => true
//@ run-call: sameEmptyLiteralSalt() => true
//@ run-call: sameFullLiteralSalt() => true
//@ run-call: differentLiteralSalts() => true

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
