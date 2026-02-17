#ifndef SHM_SHIM_H
#define SHM_SHIM_H

#include <sys/mman.h>
#include <fcntl.h>
#include <unistd.h>

/// Wrapper around shm_open since Swift can't call variadic C functions.
static inline int shm_open_fixed(const char *name, int oflag, mode_t mode) {
    return shm_open(name, oflag, mode);
}

#endif /* SHM_SHIM_H */
