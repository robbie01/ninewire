#include "udt.h"
#include "rust/cxx.h"

namespace UDT {
    using c_void = void;

    UDT_API int select_single(UDTSOCKET u, bool writable);
}