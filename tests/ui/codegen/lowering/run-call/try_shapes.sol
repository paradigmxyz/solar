//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: test_try => 1042

contract Callee {
    function f(uint x) external pure returns (uint) {
        return x * 2;
    }
    function g(uint x) external pure returns (uint, uint) {
        return (x, x + 1);
    }
}

contract TryShapes {
    Callee callee;

    constructor() {
        callee = new Callee();
    }

    function ok() external pure returns (uint) {
        return 1;
    }
    function bad() external pure returns (uint) {
        revert("boom");
    }

    function test_try() external returns (uint) {
        uint r;
        try this.ok() returns (uint v) {
            r = v;
        } catch Error(string memory) {
            r = 100;
        }
        try this.bad() returns (uint v) {
            r += v;
        } catch Error(string memory reason) {
            r += 200 + bytes(reason).length;
        } catch {
            r += 300;
        }
        try callee.f(3) returns (uint v) {
            r += v;
        } catch Panic(uint) {
            r += 400;
        } catch (bytes memory) {
            r += 500;
        }
        try callee.g(4) returns (uint a, uint b) {
            r += a + b;
        } catch {
            r += 600;
        }
        try callee.f{gas: 100000}(5) returns (uint v) {
            r += v;
        } catch {
            r += 700;
        }
        try new Callee() returns (Callee c) {
            r += 800;
        } catch {
            r += 900;
        }
        function(uint) external returns (uint) fptr = callee.f;
        try fptr(6) returns (uint v) {
            r += v;
        } catch {
            r += 1000;
        }
        return r;
    }
}
