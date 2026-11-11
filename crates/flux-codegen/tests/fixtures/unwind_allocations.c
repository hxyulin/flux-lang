#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

struct slot { void *pointer; size_t epoch; };
static struct slot slots[16384];
static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static size_t epoch, allocations, releases;

static void fail(const char *message) {
    fprintf(stderr, "allocation probe: %s\n", message);
    _exit(86);
}
static size_t find(void *pointer) {
    for (size_t i = 0; i < 16384; ++i)
        if (slots[i].pointer == pointer) return i;
    fail("free of an unknown or already freed allocation");
    return 0;
}
static void allocated(void *pointer) {
    if (!pointer) return;
    pthread_mutex_lock(&lock);
    for (size_t i = 0; i < 16384; ++i)
        if (slots[i].pointer == pointer) fail("duplicate live allocation");
    for (size_t i = 0; i < 16384; ++i) {
        if (!slots[i].pointer) {
            slots[i] = (struct slot){pointer, epoch};
            if (epoch) ++allocations;
            pthread_mutex_unlock(&lock);
            return;
        }
    }
    fail("tracking capacity exhausted");
}
static void released(void *pointer) {
    if (!pointer) return;
    pthread_mutex_lock(&lock);
    size_t index = find(pointer);
    if (epoch && slots[index].epoch == epoch) ++releases;
    slots[index].pointer = NULL;
    pthread_mutex_unlock(&lock);
}
void *flux_malloc_probe(size_t size) {
    void *pointer = malloc(size);
    allocated(pointer);
    return pointer;
}
void *flux_calloc_probe(size_t count, size_t size) {
    void *pointer = calloc(count, size);
    allocated(pointer);
    return pointer;
}
void *flux_realloc_probe(void *pointer, size_t size) {
    void *result = realloc(pointer, size);
    if (result) { released(pointer); allocated(result); }
    return result;
}
void flux_free_probe(void *pointer) {
    if (epoch && getenv("PROBE_OMIT_FREE")) return;
    released(pointer);
    free(pointer);
}
int flux_posix_memalign_probe(void **out, size_t align, size_t size) {
    int result = posix_memalign(out, align, size);
    if (!result) allocated(*out);
    return result;
}
void *probe_layout_alloc(size_t size, size_t align, int zero) {
    if (align <= 16 && align <= size)
        return zero ? flux_calloc_probe(1, size) : flux_malloc_probe(size);
    void *pointer = NULL;
    if (flux_posix_memalign_probe(&pointer, align < 8 ? 8 : align, size)) return NULL;
    if (zero) memset(pointer, 0, size);
    return pointer;
}
void *probe_layout_realloc(void *pointer, size_t old_size, size_t align, size_t size) {
    if (align <= 16 && align <= size) return flux_realloc_probe(pointer, size);
    void *result = probe_layout_alloc(size, align, 0);
    if (result) { memcpy(result, pointer, old_size < size ? old_size : size); flux_free_probe(pointer); }
    return result;
}
void probe_begin(void) {
    ++epoch;
    allocations = releases = 0;
}
int probe_end(void) {
    size_t live = 0;
    for (size_t i = 0; i < 16384; ++i)
        if (slots[i].pointer && slots[i].epoch == epoch) ++live;
    int okay = allocations && !live && allocations == releases;
    if (!okay) fprintf(stderr, "allocation probe: %zu allocated, %zu released, %zu outstanding\n", allocations, releases, live);
    return okay;
}
