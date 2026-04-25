#include "platform.h"
#include "fortias_core.h"

FortiasResult fortias_rng_bytes(uint8_t* buf, size_t len) {
    if (buf == NULL) {
        if (len > 0)
            return FORTIAS_ERR_BAD_INPUT;
        return FORTIAS_OK;
    }

#ifdef _WIN32
    {
        BCRYPT_ALG_HANDLE hAlg = 0;
        NTSTATUS          status = BCryptOpenProvider(&hAlg, NULL, 0);
        if (status != 0 || hAlg == 0)
            return FORTIAS_ERR_INTERNAL;

        status = BCryptGenRandom(hAlg, buf, len, 0);
        BCryptCloseProvider(hAlg, 0);

        if (status != 0)
            return FORTIAS_ERR_INTERNAL;
        return FORTIAS_OK;
    }
#else
    {
        int fd = open("/dev/urandom", O_RDONLY);
        if (fd < 0)
            return FORTIAS_ERR_INTERNAL;

        size_t remaining = len;
        while (remaining > 0) {
            ssize_t n = read(fd, buf, remaining);
            if (n < 0) {
                if (errno == EINTR)
                    continue;
                close(fd);
                return FORTIAS_ERR_INTERNAL;
            }
            if (n == 0) {
                /* /dev/urandom should not return 0, but guard anyway */
                close(fd);
                return FORTIAS_ERR_INTERNAL;
            }
            buf += n;
            remaining -= (size_t)n;
        }

        close(fd);
        return FORTIAS_OK;
    }
#endif
}
