function f() {
    break; //~ ERROR: `break` outside of a loop
    continue; //~ ERROR: `continue` outside of a loop

    for (uint256 i = 0; i < 10; i++) {
        break;
        continue;
        {
            break;
            continue;
        }
    }

    while (true) {
        break;
        continue;
        {
            break;
            continue;
        }
    }

    do {
        break;
        continue;
        {
            break;
            continue;
        }
    } while (true);

    {
        break; //~ ERROR: `break` outside of a loop
        continue; //~ ERROR: `continue` outside of a loop

        for (uint256 i = 0; i < 10; i++) {
            break;
            continue;
            {
                break;
                continue;
            }
        }

        while (true) {
            break;
            continue;
            {
                break;
                continue;
            }
        }

        do {
            break;
            continue;
            {
                break;
                continue;
            }
        } while (true);
    }
}

contract C {
    function f() {
        break; //~ ERROR: `break` outside of a loop
        continue; //~ ERROR: `continue` outside of a loop

        for (uint256 i = 0; i < 10; i++) {
            break;
            continue;
            {
                break;
                continue;
            }
        }

        while (true) {
            break;
            continue;
            {
                break;
                continue;
            }
        }

        do {
            break;
            continue;
            {
                break;
                continue;
            }
        } while (true);

        {
            break; //~ ERROR: `break` outside of a loop
            continue; //~ ERROR: `continue` outside of a loop
    
            for (uint256 i = 0; i < 10; i++) {
                break;
                continue;
                {
                    break;
                    continue;
                }
            }
    
            while (true) {
                break;
                continue;
                {
                    break;
                    continue;
                }
            }
    
            do {
                break;
                continue;
                {
                    break;
                    continue;
                }
            } while (true);
        }
    }
}
