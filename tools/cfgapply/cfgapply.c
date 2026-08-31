/*
 * cfgapply -- make edits to /mnt/sd/Config/viofo_config.ini take effect.
 *
 * cardv writes that file but never reads it back; the parser it was built with
 * (Menu_LoadString) has no callers. See cardv-re.md section 4. This LD_PRELOAD
 * shim supplies the missing half:
 *
 *   1. Its constructor reads the ini into memory immediately, before cardv has
 *      a chance to overwrite the file with its own current settings.
 *   2. A thread waits until cardv has finished loading settings out of PStore
 *      -- detected by the SYSP blob's magic and length appearing at the global
 *      it lives behind -- and only then applies the parsed values.
 *   3. Values are applied by calling cardv's own set_setting(id, value).
 *
 * No libc function is interposed, so no dlsym and no libdl is needed.
 *
 * EVERY ADDRESS BELOW IS SPECIFIC TO ONE BUILD: the A329S image whose u-boot
 * build tag is 20260815 (FW_VERSION_NUM = VIOFO_A329S_V2.2_260815). cardv is
 * not position independent, so the addresses are absolute and stable for that
 * build -- and meaningless for any other. Check before using this elsewhere.
 *
 * UNTESTED ON HARDWARE. It compiles and the addresses are derived from static
 * analysis; nobody has yet watched it run on a camera.
 */
#define _GNU_SOURCE
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <stdint.h>

#include "settings_table.h"

/* --- build-specific addresses, from cardv-re.md ------------------------- */
#define SET_SETTING_ADDR   0x4515c0UL   /* void set_setting(int id, int val) */
#define SYSP_PTR_ADDR      0x10443a8UL  /* void **, -> the 0x6f8-byte blob    */
#define SYSP_MAGIC         0xAAAAAAAAu  /* blob[0x000]                        */
#define SYSP_LEN           0x6f8u       /* blob[0x0a4]                        */

#define INI_PATH "/mnt/sd/Config/viofo_config.ini"
#define MAX_SET  128
#define WAIT_MS  30000                  /* give up after 30 s                 */

typedef void (*set_setting_fn)(int, int);

static struct { int id; int val; } g_pending[MAX_SET];
static int g_npending;

static int lookup_id(const char *key)
{
    for (int i = 0; i < CFG_TABLE_N; i++)
        if (strcmp(CFG_TABLE[i].key, key) == 0)
            return CFG_TABLE[i].id;
    return -1;
}

/* Parse "Key=Value" lines; ignore [sections], # comments and quoted values
 * (text settings are not applied -- they live in buffers, not setting ids). */
static void parse_ini(void)
{
    FILE *f = fopen(INI_PATH, "r");
    if (!f) {
        fprintf(stderr, "cfgapply: no %s\n", INI_PATH);
        return;
    }
    char line[512];
    while (fgets(line, sizeof line, f)) {
        char *p = line;
        while (*p == ' ' || *p == '\t') p++;
        if (*p == '#' || *p == '[' || *p == '\r' || *p == '\n' || !*p)
            continue;
        char *eq = strchr(p, '=');
        if (!eq)
            continue;
        *eq = '\0';
        char *key = p, *val = eq + 1;
        for (char *e = eq - 1; e >= key && (*e == ' ' || *e == '\t'); e--)
            *e = '\0';
        while (*val == ' ' || *val == '\t') val++;
        if (*val == '"')                       /* text setting -- skip */
            continue;
        int id = lookup_id(key);
        if (id < 0) {
            fprintf(stderr, "cfgapply: unknown key \"%s\"\n", key);
            continue;
        }
        if (g_npending >= MAX_SET)
            break;
        g_pending[g_npending].id = id;
        g_pending[g_npending].val = (int)strtol(val, NULL, 10);
        g_npending++;
    }
    fclose(f);
    fprintf(stderr, "cfgapply: parsed %d setting(s) from %s\n", g_npending, INI_PATH);
}

static int settings_ready(void)
{
    const volatile uint8_t *blob = *(uint8_t * volatile *)SYSP_PTR_ADDR;
    if (!blob)
        return 0;
    return *(const volatile uint32_t *)(blob + 0x000) == SYSP_MAGIC
        && *(const volatile uint32_t *)(blob + 0x0a4) == SYSP_LEN;
}

static void *apply_thread(void *unused)
{
    (void)unused;
    for (int waited = 0; waited < WAIT_MS; waited += 50) {
        if (settings_ready()) {
            set_setting_fn set_setting = (set_setting_fn)SET_SETTING_ADDR;
            for (int i = 0; i < g_npending; i++)
                set_setting(g_pending[i].id, g_pending[i].val);
            fprintf(stderr, "cfgapply: applied %d setting(s)\n", g_npending);
            return NULL;
        }
        usleep(50 * 1000);
    }
    fprintf(stderr, "cfgapply: settings blob never became ready; applied nothing\n");
    return NULL;
}

__attribute__((constructor))
static void cfgapply_init(void)
{
    parse_ini();                 /* read the file before cardv rewrites it */
    if (g_npending == 0)
        return;
    pthread_t t;
    if (pthread_create(&t, NULL, apply_thread, NULL) == 0)
        pthread_detach(t);
}
