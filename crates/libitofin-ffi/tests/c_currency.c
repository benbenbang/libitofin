#include "itofin.h"
#include <assert.h>
#include <string.h>

int main(void) {
    struct ItofinContext *ctx = NULL;
    struct ItofinError error;
    struct ItofinCurrencyConfig cfg;
    memset(&cfg, 0, sizeof(cfg));
    const uint8_t name[] = {'N', 0, 0xe5, 0x85, 0x83};
    const uint8_t code[] = {'T', 'W', 'D'};
    const uint8_t symbol[] = {'N', 'T', '$'};
    cfg.name = name;
    cfg.name_length = sizeof(name);
    cfg.code = code;
    cfg.code_length = sizeof(code);
    cfg.numeric_code = 901;
    cfg.symbol = symbol;
    cfg.symbol_length = sizeof(symbol);
    cfg.fractions_per_unit = 100;
    uint64_t id = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_currency_custom_new(ctx, &cfg, &id, &error) == 0);
    size_t required = 0;
    assert(itofin_currency_text(ctx, id, 0, NULL, 0, &required, &error) == 0);
    assert(required == sizeof(name));
    uint8_t output[8];
    memset(output, 99, sizeof(output));
    assert(itofin_currency_text(ctx, id, 0, output, sizeof(name), &required, &error) == 0);
    assert(memcmp(output, name, sizeof(name)) == 0 && output[sizeof(name)] == 99);
    required = 91;
    assert(itofin_currency_text(ctx, id, 0, output, 1, &required, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(required == 91 && memcmp(output, name, sizeof(name)) == 0);
    assert(itofin_currency_code(ctx, id, output, 4, &error) == 0);
    assert(memcmp(output, "TWD\0", 4) == 0);
    int32_t integer = 0;
    assert(itofin_currency_integer(ctx, id, 0, &integer, &error) == 0 && integer == 901);
    assert(itofin_currency_integer(ctx, id, 1, &integer, &error) == 0 && integer == 100);
    assert(itofin_handle_release(ctx, id, &error) == 0);
    assert(itofin_currency_integer(ctx, id, 0, &integer, &error) == ITOFIN_INVALID_HANDLE);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
