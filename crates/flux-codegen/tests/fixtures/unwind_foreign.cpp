#include <cstdio>

extern "C" int cpp_forward(void (*callback)(), int swallow) {
    try {
        callback();
        return 11;
    } catch (...) {
        if (swallow) return 23;
        throw;
    }
}

struct Foreign {
    ~Foreign() { std::fputs("event foreign destroyed\n", stderr); }
};
extern "C" void cpp_throw() { throw Foreign{}; }
