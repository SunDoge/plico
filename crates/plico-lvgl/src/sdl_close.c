/* Keep LVGL's SDL event loop from deleting objects still owned by Adapter.
 * Event filters may run on another SDL thread, so use a C11 atomic flag. */
#include <SDL.h>
#include <stdatomic.h>
#include <stdbool.h>

static atomic_bool closing;

static int filter_close(void *unused, SDL_Event *event) {
    (void)unused;
    if (event->type == SDL_QUIT ||
        (event->type == SDL_WINDOWEVENT && event->window.event == SDL_WINDOWEVENT_CLOSE)) {
        atomic_store(&closing, true);
        return 0;
    }
    return 1;
}

void plico_sdl_install_close_filter(void) {
    atomic_store(&closing, false);
    SDL_SetEventFilter(filter_close, NULL);
}

int plico_sdl_close_requested(void) {
    return atomic_load(&closing);
}
