// Xvfb-only input-to-pixel probe. Click the replay control, watch the detail labels.
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/extensions/XTest.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static double now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000.0 + ts.tv_nsec / 1000000.0;
}

int main(int argc, char **argv) {
    if (argc != 7) {
        fprintf(stderr, "usage: visible CLICK_X CLICK_Y REGION_X REGION_Y WIDTH HEIGHT\n");
        return 1;
    }
    Display *display = XOpenDisplay(NULL);
    if (!display) return 1;
    Window root = DefaultRootWindow(display);
    int x = atoi(argv[3]), y = atoi(argv[4]);
    unsigned width = atoi(argv[5]), height = atoi(argv[6]);
    XTestFakeMotionEvent(display, -1, atoi(argv[1]), atoi(argv[2]), 0);
    XSync(display, False);
    for (int i = 0; i < 110; i++) {
        struct timespec settle = {0, 50000000};
        nanosleep(&settle, NULL);
        XImage *before = XGetImage(display, root, x, y, width, height, AllPlanes, ZPixmap);
        if (!before) return 1;
        double start = now_ms();
        XTestFakeButtonEvent(display, 1, True, 0);
        XTestFakeButtonEvent(display, 1, False, 0);
        XFlush(display);
        double elapsed;
        for (;;) {
            XImage *after = XGetImage(display, root, x, y, width, height, AllPlanes, ZPixmap);
            if (!after) return 1;
            elapsed = now_ms() - start;
            int changed = memcmp(before->data, after->data, before->bytes_per_line * height);
            XDestroyImage(after);
            if (changed) break;
            if (elapsed > 1000) {
                fprintf(stderr, "No visible change after event %d\n", i);
                return 2;
            }
            struct timespec pause = {0, 500000};
            nanosleep(&pause, NULL);
        }
        XDestroyImage(before);
        if (i >= 10) printf("%.3f\n", elapsed);
    }
    XCloseDisplay(display);
}
