//@ revisions: paris shanghai
//@[paris] compile-flags: --evm-version paris
//@[shanghai] compile-flags: --evm-version shanghai
//@[paris] run-call-fail: 0x
//@[shanghai] run-call: 0x => 0x0000000000000000000000000000000000000000000000000000000000000000

contract ForkRuntime {
    constructor() {
        bytes memory runtime = hex"5f60005260206000f3";
        assembly {
            return(add(runtime, 0x20), mload(runtime))
        }
    }
}
