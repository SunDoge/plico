#pragma once
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
// Same GUI task, LVGL lock held. BSP initializes LVGL/display/touch first.
// Mounts under the active screen. 0 success; -1 viewport; -2 lifecycle;
// -3 declaration; -4 LVGL projection. One instance only.
int32_t plico_p4_create(uint32_t width, uint32_t height);
// Call from a GUI timer after input callbacks. 0 idle, 1 declared, negative error.
int32_t plico_p4_update(uint32_t time_ms);
// Diagnostic entry points, same GUI task requirements.
uint32_t plico_p4_count(void);
int32_t plico_p4_self_test(void);
// Stop update timer first; destroy before deleting the active screen.
void plico_p4_destroy(void);
#ifdef __cplusplus
}
#endif
