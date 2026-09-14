/* Compile and execute as both C and C++ against the generated public header. */
#include "itofin.h"
#include <assert.h>
#include <string.h>

int main(void) {
    ItofinContext *ctx = NULL;
    ItofinError error;
    int32_t serial = -1;
    uint64_t settings = 0;
    assert(itofin_abi_version() == 1);
    assert(strlen(itofin_version()) > 0);
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_date_new(31, 2, 2024, &serial, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(error.code == ITOFIN_INVALID_ARGUMENT && error.message[0] != '\0');
    assert(serial == -1);
    assert(itofin_date_new(29, 2, 2024, &serial, &error) == 0);
    assert(error.code == 0 && error.message[0] == '\0');
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, serial, &error) == 0);
    assert(itofin_handle_release(ctx, settings, &error) == 0);
    assert(itofin_handle_release(ctx, settings, &error) == ITOFIN_INVALID_HANDLE);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
