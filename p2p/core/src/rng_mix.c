#include "platform.h"
#include "foretias_core.h"

ForetiasResult foretias_rng_bytes(uint8_t* buf, size_t len) {
    if (buf == NULL) {
        if (len > 0)
            return FORETIAS_ERR_BAD_INPUT;
        return FORETIAS_OK;
    }

#ifdef _WIN32
    {
        BCRYPT_ALG_HANDLE hAlg = 0;
        NTSTATUS          status = BCryptOpenProvider(&hAlg, NULL, 0);
        if (status != 0 || hAlg == 0)
            return FORETIAS_ERR_INTERNAL;

        status = BCryptGenRandom(hAlg, buf, len, 0);
        BCryptCloseProvider(hAlg, 0);

        if (status != 0)
            return FORETIAS_ERR_INTERNAL;
        return FORETIAS_OK;
    }
#else
    {
        int fd = open("/dev/urandom", O_RDONLY);
        if (fd < 0)
            return FORETIAS_ERR_INTERNAL;

        size_t remaining = len;
        while (remaining > 0) {
            ssize_t n = read(fd, buf, remaining);
            if (n < 0) {
                if (errno == EINTR)
                    continue;
                close(fd);
                return FORETIAS_ERR_INTERNAL;
            }
            if (n == 0) {
                /* /dev/urandom should not return 0, but guard anyway */
                close(fd);
                return FORETIAS_ERR_INTERNAL;
            }
            buf += n;
            remaining -= (size_t)n;
        }

        close(fd);
        return FORETIAS_OK;
    }
#endif
}
