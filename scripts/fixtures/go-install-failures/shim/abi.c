#include <stdint.h>

#ifdef __APPLE__
extern uint32_t itofin_abi_version(void);

static uint32_t incompatible_abi(void) {
    return UINT32_MAX;
}

__attribute__((used, section("__DATA,__interpose")))
static struct {
    uint32_t (*replacement)(void);
    uint32_t (*original)(void);
} interpose_abi = {incompatible_abi, itofin_abi_version};
#else
uint32_t itofin_abi_version(void) {
    return UINT32_MAX;
}
#endif
