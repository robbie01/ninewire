#include "bridge.h"

#include <algorithm>
#include <iterator>

namespace UDT {
    int epoll_wait3(int eid, rust::Vec<UDTSOCKET> *readfds, rust::Vec<UDTSOCKET> *writefds, int64_t msTimeOut, rust::Vec<SYSSOCKET> *lrfds, rust::Vec<SYSSOCKET> *lwfds) {
        std::set<UDTSOCKET> readfds_set;
        std::set<UDTSOCKET> writefds_set;
        std::set<SYSSOCKET> lrfds_set;
        std::set<SYSSOCKET> lwfds_set;
        
        int result = epoll_wait(
            eid,
            readfds ? &readfds_set : nullptr,
            writefds ? &writefds_set : nullptr,
            msTimeOut,
            lrfds ? &lrfds_set : nullptr,
            lwfds ? &lwfds_set : nullptr
        );

        if (readfds) {
            readfds->clear();
            std::copy(readfds_set.begin(), readfds_set.end(), std::back_inserter(*readfds));
        }

        if (writefds) {
            writefds->clear();
            std::copy(writefds_set.begin(), writefds_set.end(), std::back_inserter(*writefds));
        }

        if (lrfds) {
            lrfds->clear();
            std::copy(lrfds_set.begin(), lrfds_set.end(), std::back_inserter(*lrfds));
        }

        if (lwfds) {
            lwfds->clear();
            std::copy(lwfds_set.begin(), lwfds_set.end(), std::back_inserter(*lwfds));
        }

        return result;
    }
}