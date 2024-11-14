contract C {
    function f() {
        break; //~ ERROR: 'break' has to be in a 'for' or 'while' loop
        continue; //~ ERROR: 'continue' has to be in a 'for' or 'while' loop

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
