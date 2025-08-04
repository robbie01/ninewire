#include "udt.h"
#include "rust/cxx.h"

namespace UDT {
    using c_void = void;

    UDT_API int epoll_wait3(int eid, rust::Vec<UDTSOCKET> *readfds, rust::Vec<UDTSOCKET> *writefds, int64_t msTimeOut, rust::Vec<SYSSOCKET> *lrfds, rust::Vec<SYSSOCKET> *wrfds);
    UDT_API int select_single(UDTSOCKET u, bool writable);
}