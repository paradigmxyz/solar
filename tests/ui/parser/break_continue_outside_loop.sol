contract C {
    function f() {
        break; //~ ERROR: `break` outside of a loop
        continue; //~ ERROR: `break` outside of a loop

        for (uint256 i = 0; i < 10; i++) {
            break;
            continue;
        }

        while (true) {
            break;
            continue;
        }

        do {
            break;
            continue;
        } while (true);
    }
}
