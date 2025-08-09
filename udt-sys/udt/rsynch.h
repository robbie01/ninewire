#pragma once

#if defined(WINDOWS)
#include <windows.h>
#include <limits>
#elif defined(MACOSX)
#include <dispatch/dispatch.h>
#else
#include <semaphore.h>
#include <time.h>
#endif

#include <mutex>
#include <condition_variable>
#include <cstdint>
#include <chrono>

namespace rsynch {

class Semaphore {
private:
#if defined(WINDOWS)
    HANDLE inner;
#elif defined(MACOSX)
    dispatch_semaphore_t inner;
#else
    sem_t inner;
#endif

public:
    Semaphore(const Semaphore&) = delete;
    Semaphore& operator=(const Semaphore&) = delete;

    inline Semaphore() {
#if defined(WINDOWS)
        inner = CreateSemaphore(nullptr, 0, LONG_MAX, nullptr);
#elif defined(MACOSX)
        inner = dispatch_semaphore_create(0);
#else
        sem_init(&inner, 0, 0);
#endif
    }

    inline ~Semaphore() {
#if defined(WINDOWS)
        CloseHandle(inner);
#elif defined(MACOSX)
        dispatch_release(inner);
#else
        sem_destroy(&inner);
#endif
    }

    inline void post() {
#if defined(WINDOWS)
        ReleaseSemaphore(inner, 1, nullptr);
#elif defined(MACOSX)
        dispatch_semaphore_signal(inner);
#else
        sem_post(&inner);
#endif
    }

    inline void wait() {
#if defined(WINDOWS)
        WaitForSingleObject(inner, INFINITE);
#elif defined(MACOSX)
        dispatch_semaphore_wait(inner, DISPATCH_TIME_FOREVER);
#else
        sem_wait(&inner);
#endif
    }

    template<class Rep, class Period>
    inline bool wait_for(const std::chrono::duration<Rep, Period>& timeout) {
#if defined(WINDOWS)
        auto delta = std::chrono::duration_cast<std::chrono::duration<DWORD, std::milli>>(timeout);
        return WaitForSingleObject(inner, delta.count()) == WAIT_OBJECT_0;
#elif defined(MACOSX)
        auto delta = std::chrono::duration_cast<std::chrono::duration<std::int64_t, std::nano>>(timeout);
        return dispatch_semaphore_wait(inner, dispatch_time(DISPATCH_TIME_NOW, delta.count())) == 0;
#else
        auto delta = std::chrono::duration_cast<std::chrono::duration<std::uint64_t, std::nano>>(timeout).count();
        timespec ts;
        clock_gettime(CLOCK_REALTIME, &ts);
        delta += ts.tv_nsec;
        ts.tv_sec += delta / 1000000000;
        ts.tv_nsec = delta % 1000000000;
        int res;
        do {
                res = sem_timedwait(&inner, &ts);
        } while (res == -1 && errno == EINTR);
        return res == 0;
#endif
    }

    inline bool try_wait() {
#if defined(WINDOWS)
        return WaitForSingleObject(inner, 0) == WAIT_OBJECT_0;
#elif defined(MACOSX)
        return dispatch_semaphore_wait(inner, DISPATCH_TIME_NOW) == 0;
#else
        return sem_trywait(&inner) == 0;
#endif
    }
};

class AutoResetEvent {
private:
    bool is_set = false;
    std::mutex mtx;
    std::condition_variable cv;

public:
    AutoResetEvent() = default;
    AutoResetEvent(const AutoResetEvent&) = delete;
    AutoResetEvent& operator=(const AutoResetEvent&) = delete;

    inline void set() {
        {
                std::lock_guard<std::mutex> lock(mtx);
                is_set = true;
        }
        cv.notify_one();
    }

    inline void wait() {
        std::unique_lock<std::mutex> lock(mtx);
        while (!is_set) cv.wait(lock);
        is_set = false;
    }

    template<class Rep, class Period>
    inline bool wait_for(const std::chrono::duration<Rep, Period>& timeout) {
        auto deadline = std::chrono::steady_clock::now() + timeout;
        std::unique_lock<std::mutex> lock(mtx);
        if (!cv.wait_until(lock, deadline, [&] { return is_set; })) return false;
        is_set = false;
        return true;
    }

    inline bool try_wait() {
        std::lock_guard<std::mutex> lock(mtx);
        if (is_set) {
                is_set = false;
                return true;
        }
        return false;
    }
};

}