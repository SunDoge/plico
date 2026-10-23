#pragma once
#include <cmath>

// Diagnostic only: reuse Plico's actual native tree. While native animation
// owns dimensions, do not call Rust update/sync or dispatch queued UI events.
static lv_obj_t *bench_bars[12]={}, *bench_progress=nullptr, *bench_badge=nullptr;
static unsigned bench_count=0, native_frames=0;
static int32_t native_last=-40;
static int64_t native_us=0, native_max_us=0;
static int native_token=0;

static int32_t native_path(const lv_anim_t *animation) {
    // Built-in linear path quantizes a long duration to 1024 steps. Preserve
    // millisecond time here so this comparison really updates around 25 Hz.
    return animation->act_time;
}

static void bench_find(lv_obj_t *obj) {
    auto parent=lv_obj_get_parent(obj);
    if (parent && lv_obj_get_width(obj)==24 && lv_obj_get_height(parent)==136 && bench_count<12)
        bench_bars[bench_count++]=obj;
    if (parent && lv_obj_get_height(obj)==14 && lv_obj_get_height(parent)==14 &&
        lv_obj_get_width(parent)==400) bench_progress=obj;
    for (unsigned i=0;i<lv_obj_get_child_count(obj);++i) bench_find(lv_obj_get_child(obj,i));
}

static void native_exec(void *, int32_t value) {
    if (value-native_last<40) return; // Match Rust's ~25 Hz, not LVGL's ~50 Hz tick.
    native_last=value;
    auto begin=esp_timer_get_time();
    for (unsigned i=0;i<12;++i) {
        float wave=(sinf(float(value)*0.0035f+float(i)*0.55f)+1.f)*0.5f;
        lv_obj_set_height(bench_bars[i],int32_t(20.f+wave*108.f));
    }
    float pulse=(sinf(float(value)*0.0017f)+1.f)*0.5f;
    lv_obj_set_width(bench_progress,int32_t(20.f+pulse*380.f));
    auto elapsed=esp_timer_get_time()-begin;
    native_us+=elapsed;
    if (elapsed>native_max_us) native_max_us=elapsed;
    ++native_frames;
}

static unsigned bench_select(int64_t since_mount) {
    // Warm up + Plico for 22s, native for 20s, static for 10s, then restore Plico.
    unsigned desired=since_mount<22000000?0:since_mount<42000000?1:since_mount<52000000?2:0;
    if (!bench_badge) {
        bench_badge=lv_label_create(lv_screen_active());
        lv_obj_set_style_text_color(bench_badge,lv_color_hex(0xffffff),0);
        lv_obj_set_style_bg_color(bench_badge,lv_color_hex(0x18334d),0);
        lv_obj_set_style_bg_opa(bench_badge,LV_OPA_COVER,0);
        lv_label_set_text(bench_badge,"BENCH / PLICO 25 Hz");
        lv_obj_align(bench_badge,LV_ALIGN_BOTTOM_MID,0,-8);
    }
    if (desired==animation_mode.load()) return desired;
    lv_anim_delete(&native_token,native_exec);
    if (desired==1) {
        bench_count=0; bench_progress=nullptr;
        bench_find(lv_screen_active());
        if (bench_count!=12 || !bench_progress) {
            ESP_LOGE(TAG,"BENCH target mismatch: bars=%u progress=%p; staying with Plico",bench_count,bench_progress);
            return 0;
        }
        native_frames=0; native_us=native_max_us=0; native_last=-40;
        lv_anim_t animation;
        lv_anim_init(&animation);
        lv_anim_set_var(&animation,&native_token);
        lv_anim_set_values(&animation,0,600000);
        lv_anim_set_duration(&animation,600000);
        lv_anim_set_path_cb(&animation,native_path);
        lv_anim_set_exec_cb(&animation,native_exec);
        lv_anim_start(&animation);
    }
    lv_label_set_text(bench_badge,desired==0?"BENCH / PLICO 25 Hz":desired==1?"BENCH / LVGL ANIM 25 Hz":"BENCH / STATIC");
    lv_obj_align(bench_badge,LV_ALIGN_BOTTOM_MID,0,-8);
    animation_mode.store(desired);
    ESP_LOGI(TAG,"BENCH mode=%u (0=Plico 1=lv_anim 2=static)",desired);
    return desired;
}
