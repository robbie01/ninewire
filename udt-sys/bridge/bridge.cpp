#include "bridge.h"

#include <algorithm>
#include <iterator>

namespace UDT {
    int select_single(UDTSOCKET u, bool writable) {
        std::vector<UDTSOCKET> fds{u};
        std::vector<UDTSOCKET> v1;
        std::vector<UDTSOCKET> v2;

        // The 500ms timeout is a **DIRTY HACK** to ensure that IO can be cancelled.
        
        // Tokio will *hang* while exiting until all `spawn_blocking`s have completed.
        // In the future, we should somehow make this remotely interruptible.
        if (writable) {
            return selectEx(fds, nullptr, &v1, &v2, 500);
        } else {
            std::vector<UDTSOCKET> v1;
            return selectEx(fds, &v1, nullptr, &v2, 500);
        }
    }
}